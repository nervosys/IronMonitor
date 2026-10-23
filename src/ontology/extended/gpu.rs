//! GPU identity and the dynamic figures the base GPU resolver omits, GPU
//! interconnect topology, PCI device extras, and the IOMMU overview.
//!
//! The adapter rows come from the same `UnifiedMonitor` enumeration that
//! `resolve_gpu` reads, index for index, so `gpu.2.uuid` describes the adapter
//! that `gpu.2.name` names. The PCI rows use the same enumeration and the same
//! `pci.<address>` segment as `resolve_pci`.

use std::collections::{HashMap, HashSet};

use crate::gpu::GpuInfo;
use crate::gpu_topology::{GpuInterconnectType, GpuTopologyMonitor, GpuTopologyNode};
use crate::pci_devices::PciDeviceMonitor;

#[allow(unused_imports)]
use super::super::resolve::*;
#[allow(unused_imports)]
use super::super::{Domain as D, Entity, EntityKind as K, Provenance as P, Unit as U};

// ── Reasons ──────────────────────────────────────────────────────────────────
//
// Each names what the reader does and does not read, because an absent value is
// only useful to an agent when it says whether a retry could help.

const UUID_WHY: &str = concat!(
    "the driver reported no UUID for this adapter. NVML publishes one for NVIDIA ",
    "cards; the AMD, Intel and Apple readers do not read one"
);
const PCI_BUS_WHY: &str = concat!(
    "no PCI address was reported for this adapter. The Apple GPU reader does ",
    "not read one, and the other readers take it from the bus enumeration"
);
const VBIOS_WHY: &str = concat!(
    "no VBIOS version was read for this adapter. Only the NVIDIA reader asks ",
    "the driver for one; the AMD, Intel and Apple readers do not"
);
const DRIVER_WHY: &str = concat!(
    "no driver version was read for this adapter. The NVIDIA reader asks NVML ",
    "for one, and the AMD reader reads the amdgpu module version on Linux"
);
const CAPABILITY_WHY: &str = concat!(
    "no compute capability was read for this adapter. NVML reports one for ",
    "NVIDIA cards; the AMD, Intel and Apple readers do not read it, and an ",
    "NVML query that failed also leaves it empty"
);
const CORES_WHY: &str = concat!(
    "no shader or core count was read for this adapter. NVML reports one for ",
    "NVIDIA cards and the Apple reader reads one where the system gives it; the ",
    "AMD and Intel readers do not read it"
);
const L2_WHY: &str = concat!(
    "no L2 cache size was read for this adapter. The NVIDIA reader's NVML ",
    "binding does not expose it, and the AMD, Intel and Apple readers do not ",
    "attempt it"
);
const ENGINES_WHY: &str = concat!(
    "no engine count was read for this adapter. Every reader in this build ",
    "leaves it empty, so this is a limit of the reader rather than of the card"
);
const FREE_WHY: &str = concat!(
    "free memory is not reported for this adapter. It is computed from the ",
    "total and used figures, and one of those was not read"
);
const MEM_UTIL_WHY: &str = concat!(
    "memory utilization is not reported for this adapter. It is computed from ",
    "the used and total figures, and one of those was not read"
);
const MEM_CLOCK_WHY: &str = concat!(
    "no memory clock was read for this adapter. NVML reports it for NVIDIA cards ",
    "and the Linux sysfs readers for AMD and Intel; the Windows AMD and Intel ",
    "backends read none"
);
const SM_CLOCK_WHY: &str = concat!(
    "no SM clock was read for this adapter. Only the NVIDIA reader asks for it, ",
    "through NVML; the other backends in this build do not read it"
);
const VIDEO_CLOCK_WHY: &str = concat!(
    "no video clock was read for this adapter. Only the NVIDIA reader asks for ",
    "it, through NVML; the other backends in this build do not read it"
);
const DEFAULT_LIMIT_WHY: &str = concat!(
    "no default power limit was read for this adapter. NVML publishes one for ",
    "NVIDIA cards; no other reader in this build asks for it"
);
const USAGE_PERCENT_WHY: &str = concat!(
    "no power usage percentage is reported for this adapter. It is computed ",
    "from the power draw and the enforced limit, and one of those was not read"
);
const FAN_SPEED_WHY: &str = concat!(
    "no fan duty was read for this adapter. NVML reports it for NVIDIA cards ",
    "and the Linux hwmon reader for AMD does too; a card with no fan, or a ",
    "reader that finds no PWM control, leaves it empty"
);
const FAN_RPM_WHY: &str = concat!(
    "no fan speed in RPM was read for this adapter. Only the Linux hwmon reader ",
    "for AMD supplies it; the NVIDIA reader does not query fan RPM"
);
const PCIE_WHY: &str = concat!(
    "this PCIe link property was not reported for this adapter. Not every GPU ",
    "backend reads the PCIe link, and this adapter's backend did not supply it"
);
const PCIE_THROUGHPUT_WHY: &str = concat!(
    "PCIe throughput was not reported for this adapter. Only backends that ",
    "sample the link's traffic counters supply it, and this one did not"
);
const ENGINE_WHY: &str = concat!(
    "engine utilization was not read for this adapter; the backend that queried ",
    "it reports no counter for this engine, which is not the same as an idle engine"
);
const VENDOR_ENGINE_NONE_WHY: &str = concat!(
    "this adapter's reader reported no vendor-specific engines, so none are ",
    "listed. Engines the driver does not name are not visible here"
);
const TOPOLOGY_NUMA_WHY: &str = concat!(
    "the NUMA node of this adapter was not read. Linux publishes it in sysfs, ",
    "and the kernel writes -1 for no affinity, which is reported as absent"
);
const TOPOLOGY_IOMMU_WHY: &str = concat!(
    "this adapter has no IOMMU group in the topology reader. The kernel links one ",
    "only when an IOMMU is enabled"
);
const TOPOLOGY_DRIVER_WHY: &str = concat!(
    "the topology reader reported no kernel driver for this adapter, which means ",
    "no driver is bound to it"
);
const TOPOLOGY_UNMATCHED_WHY: &str = concat!(
    "the topology reader found no device at this adapter's PCI address, so ",
    "there is no topology row to join it to"
);
const LINK_NONE_FEWER_WHY: &str =
    "fewer than two GPUs were enumerated, and an interconnect joins two of them";
const LINK_NONE_NO_LINKS_WHY: &str = concat!(
    "the topology reader found no interconnect between any two enumerated GPUs ",
    "on this machine"
);
const LINK_NONE_UNMAPPED_WHY: &str = concat!(
    "the topology reader reported interconnects, but none joins two GPUs that ",
    "were enumerated, so none can be named by index"
);
const LINK_TYPE_UNCLASSIFIED_WHY: &str = concat!(
    "the topology reader could not classify the interconnect between this pair, ",
    "or found none"
);
const LINK_BANDWIDTH_WHY: &str = concat!(
    "no per-link bandwidth was read for this pair. The topology reader publishes ",
    "a rate only for PCIe links; the per-link rate of NVLink and xGMI needs a ",
    "vendor library it does not query"
);
const PCI_ID_WHY: &str = "the device listing did not report this PCI identifier for the device";
const PCI_SUBSYSTEM_WHY: &str = concat!(
    "the device listing reported no subsystem identifier for this device; on ",
    "Windows it is read from the PnP instance ID, which carries one only for ",
    "devices with a subsystem"
);
const PCI_CLASS_CODE_WHY: &str = concat!(
    "this platform's device listing reports no PCI class code for the device. ",
    "Windows and macOS give only the decoded class, which pci.<address>.class holds"
);
const PCI_REVISION_WHY: &str = concat!(
    "the device listing reported no revision for this device. Windows and macOS ",
    "do not expose the PCI revision register"
);
const PCI_KERNEL_MODULE_WHY: &str = concat!(
    "the device enumerator does not read the kernel module name. The driver bound ",
    "to the device is published as pci.<address>.driver"
);
const PCI_IOMMU_GROUP_LINUX_WHY: &str = concat!(
    "the device is in no IOMMU group. The kernel links one only when an IOMMU is ",
    "enabled, so this device is outside any IOMMU domain"
);
const PCI_IOMMU_GROUP_OTHER_WHY: &str = concat!(
    "IOMMU groups are read from /sys/kernel/iommu_groups, which Windows and macOS ",
    "do not expose"
);
const PCI_SRIOV_WHY: &str = concat!(
    "SR-IOV capability was not reported for this device. The platform's device ",
    "listing does not say, and Windows and macOS do not report it"
);
const PCI_SRIOV_VFS_WHY: &str = concat!(
    "the enabled virtual function count was not read for this device. It is ",
    "published only for devices whose platform reports SR-IOV"
);
const PCI_POWER_STATE_WHY: &str = concat!(
    "no PCI power state was read for this device. Linux reads it from sysfs; ",
    "Windows reports only device health, not a power state, and macOS reports none"
);
const IOMMU_DISABLED_WHY: &str =
    "no IOMMU is enabled on this machine, so there is no IOMMU type to report";
const IOMMU_TYPE_WHY: &str = concat!(
    "an IOMMU is enabled but neither the ACPI DMAR and IVRS tables nor the kernel ",
    "command line identify its type"
);
const INTERRUPT_REMAP_WHY: &str = concat!(
    "interrupt remapping is not published. The reader checks only whether ",
    "/sys/class/iommu exists, and that path existing does not show that remapping ",
    "is active"
);

// ── Schema ───────────────────────────────────────────────────────────────────

pub(super) fn declare(add: &mut dyn FnMut(Entity)) {
    // Identity. Strings read from the driver are measured; the chip constants the
    // driver reports are specifications; `integrated` is computed from the name.
    add(Entity::new(
        "gpu.{n}.uuid",
        D::Gpu,
        K::Identity,
        Some(U::Identifier),
        P::Measured,
        true,
        "The adapter's UUID as its driver reports it. Only NVIDIA cards publish one in this build.",
    ));
    add(Entity::new(
        "gpu.{n}.pci_bus_id",
        D::Gpu,
        K::Identity,
        Some(U::Identifier),
        P::Measured,
        true,
        "The PCI address of the adapter, in the domain:bus:device.function form the driver uses. Matches the pci.<address> rows once separators are replaced.",
    ));
    add(Entity::new(
        "gpu.{n}.vbios_version",
        D::Gpu,
        K::Identity,
        Some(U::Text),
        P::Measured,
        true,
        "The video BIOS version the adapter's driver reports. Only NVIDIA cards are asked in this build.",
    ));
    add(Entity::new(
        "gpu.{n}.driver_version",
        D::Gpu,
        K::Identity,
        Some(U::Text),
        P::Measured,
        true,
        "The driver version the adapter reports. Unavailable on Intel, whose reader holds the kernel driver's name (i915, xe) rather than a version.",
    ));
    add(Entity::new(
        "gpu.{n}.compute_capability",
        D::Gpu,
        K::Identity,
        Some(U::Text),
        P::Specification,
        true,
        "The CUDA compute capability as major.minor. A property of the chip the driver declares, not a measurement of this run.",
    ));
    add(Entity::new(
        "gpu.{n}.shader_cores",
        D::Gpu,
        K::Identity,
        Some(U::Count),
        P::Specification,
        true,
        "The number of CUDA cores or shader units the driver declares for this chip. A property of the silicon, not a measurement.",
    ));
    add(Entity::new(
        "gpu.{n}.l2_cache",
        D::Gpu,
        K::Identity,
        Some(U::Bytes),
        P::Specification,
        true,
        "The size of the L2 cache in bytes, as the chip declares it. A property of the silicon, not a measurement.",
    ));
    add(Entity::new(
        "gpu.{n}.num_engines",
        D::Gpu,
        K::Identity,
        Some(U::Count),
        P::Specification,
        true,
        "The number of execution engines the chip declares. A property of the silicon, not a measurement.",
    ));
    add(Entity::new(
        "gpu.{n}.integrated",
        D::Gpu,
        K::Identity,
        None,
        P::Derived,
        true,
        concat!(
            "Whether the adapter shares system memory with the CPU. Published for Apple ",
            "silicon, where the GPU is on the SoC by construction. The other readers ",
            "guess it from the product name, so elsewhere it is unavailable."
        ),
    )
    .derived(&["gpu.{n}.vendor"]));

    // Memory. Free and utilization are computed from total and used, so they are
    // derived, not read.
    add(Entity::new(
        "gpu.{n}.memory.free",
        D::Gpu,
        K::Measurement,
        Some(U::Bytes),
        P::Derived,
        true,
        "Free video memory in bytes, computed as total minus used.",
    )
    .derived(&["gpu.{n}.memory.total", "gpu.{n}.memory.used"]));
    add(Entity::new(
        "gpu.{n}.memory.utilization",
        D::Gpu,
        K::Measurement,
        Some(U::Percent),
        P::Derived,
        true,
        "Video memory in use as a percentage of total, computed from the used and total figures.",
    )
    .derived(&["gpu.{n}.memory.used", "gpu.{n}.memory.total"]));

    // Clocks.
    add(Entity::new(
        "gpu.{n}.clocks.memory",
        D::Gpu,
        K::Measurement,
        Some(U::Megahertz),
        P::Measured,
        true,
        "The current memory clock in MHz, sampled from the driver.",
    ));
    add(Entity::new(
        "gpu.{n}.clocks.memory.max",
        D::Gpu,
        K::Measurement,
        Some(U::Megahertz),
        P::Measured,
        true,
        "The highest memory clock in MHz the driver reports for this adapter.",
    ));
    add(Entity::new(
        "gpu.{n}.clocks.sm",
        D::Gpu,
        K::Measurement,
        Some(U::Megahertz),
        P::Measured,
        true,
        "The current streaming multiprocessor clock in MHz, sampled from the driver.",
    ));
    add(Entity::new(
        "gpu.{n}.clocks.video",
        D::Gpu,
        K::Measurement,
        Some(U::Megahertz),
        P::Measured,
        true,
        "The current video engine clock in MHz, sampled from the driver.",
    ));

    // Power.
    add(Entity::new(
        "gpu.{n}.power.default_limit",
        D::Gpu,
        K::Limit,
        Some(U::Milliwatts),
        P::Specification,
        true,
        "The power limit the board ships with, in milliwatts. A property of the board the driver declares, not a sample.",
    ));
    add(Entity::new(
        "gpu.{n}.power.usage_percent",
        D::Gpu,
        K::Measurement,
        Some(U::Percent),
        P::Derived,
        true,
        "Power draw as a percentage of the enforced power limit, computed from the two.",
    )
    .derived(&["gpu.{n}.power.draw", "gpu.{n}.power.limit"]));

    // Cooling.
    add(Entity::new(
        "gpu.{n}.thermal.fan_speed",
        D::Gpu,
        K::Measurement,
        Some(U::Percent),
        P::Measured,
        true,
        "The fan duty cycle as a percentage of full speed, sampled from the driver.",
    ));
    add(Entity::new(
        "gpu.{n}.thermal.fan_rpm",
        D::Gpu,
        K::Measurement,
        Some(U::Rpm),
        P::Measured,
        true,
        "The fan speed in revolutions per minute, sampled from the hardware monitor.",
    ));

    // PCIe link of the adapter.
    add(Entity::new(
        "gpu.{n}.pcie.current_gen",
        D::Gpu,
        K::Measurement,
        Some(U::Count),
        P::Measured,
        true,
        "The PCIe generation the adapter's link is running at now, from 1 to 6.",
    ));
    add(Entity::new(
        "gpu.{n}.pcie.max_gen",
        D::Gpu,
        K::Identity,
        Some(U::Count),
        P::Measured,
        true,
        "The highest PCIe generation the adapter's link can run at, from 1 to 6.",
    ));
    add(Entity::new(
        "gpu.{n}.pcie.current_width",
        D::Gpu,
        K::Measurement,
        Some(U::Count),
        P::Measured,
        true,
        "The number of PCIe lanes the adapter's link is using now.",
    ));
    add(Entity::new(
        "gpu.{n}.pcie.max_width",
        D::Gpu,
        K::Identity,
        Some(U::Count),
        P::Measured,
        true,
        "The number of PCIe lanes the adapter's link can use at most.",
    ));
    add(Entity::new(
        "gpu.{n}.pcie.current_speed",
        D::Gpu,
        K::Measurement,
        Some(U::MegatransfersPerSecond),
        P::Measured,
        true,
        "The current PCIe link speed in megatransfers per second.",
    ));
    add(Entity::new(
        "gpu.{n}.pcie.max_speed",
        D::Gpu,
        K::Identity,
        Some(U::MegatransfersPerSecond),
        P::Measured,
        true,
        "The highest PCIe link speed the adapter supports, in megatransfers per second.",
    ));
    add(Entity::new(
        "gpu.{n}.pcie.tx_throughput",
        D::Gpu,
        K::Measurement,
        Some(U::BytesPerSecond),
        P::Measured,
        true,
        "Data the adapter sent over PCIe, in bytes per second, from the driver's traffic counters.",
    ));
    add(Entity::new(
        "gpu.{n}.pcie.rx_throughput",
        D::Gpu,
        K::Measurement,
        Some(U::BytesPerSecond),
        P::Measured,
        true,
        "Data the adapter received over PCIe, in bytes per second, from the driver's traffic counters.",
    ));

    // Engines.
    for (suffix, what) in [
        ("graphics", "3D graphics"),
        ("compute", "compute"),
        ("encoder", "video encoder"),
        ("decoder", "video decoder"),
        ("copy", "copy (DMA)"),
    ] {
        add(Entity::new(
            &format!("gpu.{{n}}.engine.{suffix}"),
            D::Gpu,
            K::Measurement,
            Some(U::Percent),
            P::Measured,
            true,
            &format!("Utilization of the {what} engine, as a percentage, sampled from the driver."),
        ));
    }
    add(Entity::new(
        "gpu.{n}.engine.vendor.{name}",
        D::Gpu,
        K::Measurement,
        Some(U::Percent),
        P::Measured,
        true,
        "Utilization of a vendor-specific engine, named by the driver, as a percentage. The segment is the engine's name with separators replaced.",
    ));
    add(Entity::new(
        "gpu.{n}.engine.vendor.<none>",
        D::Gpu,
        K::Diagnostic,
        None,
        P::Unavailable,
        true,
        "Present when an adapter reports no vendor-specific engines, carrying why none are listed.",
    ));

    // Topology. Read from sysfs on Linux; the other platforms report why it is absent.
    add(Entity::new(
        "gpu.{n}.topology.numa_node",
        D::Gpu,
        K::Identity,
        Some(U::Count),
        P::Measured,
        true,
        "The NUMA node the adapter is attached to, from the GPU topology reader.",
    ));
    add(Entity::new(
        "gpu.{n}.topology.iommu_group",
        D::Gpu,
        K::Identity,
        Some(U::Count),
        P::Measured,
        true,
        "The IOMMU group the adapter belongs to, from the GPU topology reader.",
    ));
    add(Entity::new(
        "gpu.{n}.topology.driver",
        D::Gpu,
        K::Identity,
        Some(U::Identifier),
        P::Measured,
        true,
        "The kernel driver bound to the adapter, from the GPU topology reader.",
    ));
    add(Entity::new(
        "gpu.link.<none>",
        D::Gpu,
        K::Diagnostic,
        None,
        P::Unavailable,
        true,
        "Present when no interconnect between two GPUs is reported, carrying why.",
    ));
    add(Entity::new(
        "gpu.{n}.link.{m}.type",
        D::Gpu,
        K::Identity,
        Some(U::Identifier),
        P::Derived,
        true,
        "The interconnect between two GPUs: nvlink, xgmi, pcie_peer_to_peer, pcie_through_cpu, pcie_through_host_bridge or same_board. Classified from sysfs markers and NUMA placement.",
    )
    .derived(&["gpu.{n}.topology.numa_node", "gpu.{n}.vendor"]));
    add(Entity::new(
        "gpu.{n}.link.{m}.bandwidth",
        D::Gpu,
        K::Measurement,
        Some(U::BytesPerSecond),
        P::Derived,
        true,
        "Theoretical one-direction bandwidth of a PCIe link between two GPUs, the slower end's rate. Computed from each end's PCIe generation and width.",
    )
    .derived(&["gpu.{n}.pcie.current_gen", "gpu.{n}.pcie.current_width"]));

    // PCI device extras, on the same pci.<address> prefix as the base rows.
    for (suffix, unit, desc) in [
        (
            "vendor_id",
            U::Identifier,
            "The PCI vendor ID, four hex digits.",
        ),
        (
            "device_id",
            U::Identifier,
            "The PCI device ID, four hex digits.",
        ),
        (
            "subsystem_vendor_id",
            U::Identifier,
            "The subsystem vendor ID the device reports, four hex digits.",
        ),
        (
            "subsystem_device_id",
            U::Identifier,
            "The subsystem device ID the device reports, four hex digits.",
        ),
        (
            "class_code",
            U::Identifier,
            "The PCI base class code, two hex digits.",
        ),
        (
            "revision",
            U::Identifier,
            "The PCI revision the device reports, as hex.",
        ),
        (
            "kernel_module",
            U::Identifier,
            "The kernel module that handles this device.",
        ),
    ] {
        add(Entity::new(
            &format!("pci.{{addr}}.{suffix}"),
            D::Pci,
            K::Identity,
            Some(unit),
            P::Measured,
            true,
            desc,
        ));
    }
    add(Entity::new(
        "pci.{addr}.iommu_group",
        D::Pci,
        K::Identity,
        Some(U::Count),
        P::Measured,
        true,
        "The IOMMU group the device belongs to. Devices in one group are isolated together for passthrough.",
    ));
    add(Entity::new(
        "pci.{addr}.sriov_capable",
        D::Pci,
        K::Identity,
        None,
        P::Measured,
        true,
        "Whether the device supports single-root I/O virtualization, as its platform reports it.",
    ));
    add(Entity::new(
        "pci.{addr}.sriov_vfs",
        D::Pci,
        K::Measurement,
        Some(U::Count),
        P::Measured,
        true,
        "The number of SR-IOV virtual functions currently enabled on the device.",
    ));
    add(Entity::new(
        "pci.{addr}.power_state",
        D::Pci,
        K::Measurement,
        Some(U::Identifier),
        P::Measured,
        true,
        "The device's PCI power state, such as D0 or D3hot, from the kernel.",
    ));

    // IOMMU overview.
    add(Entity::new(
        "system.iommu.enabled",
        D::System,
        K::Measurement,
        None,
        P::Measured,
        true,
        "Whether an IOMMU is enabled, from the kernel's IOMMU groups.",
    ));
    add(Entity::new(
        "system.iommu.type",
        D::System,
        K::Identity,
        Some(U::Identifier),
        P::Measured,
        true,
        "The IOMMU technology: intel_vtd, amd_vi, arm_smmu or apple_dart.",
    ));
    add(Entity::new(
        "system.iommu.interrupt_remapping",
        D::System,
        K::Measurement,
        None,
        P::Measured,
        true,
        "Whether interrupt remapping is active, which secure device passthrough needs.",
    ));
}

// ── Resolution ───────────────────────────────────────────────────────────────

pub(super) fn resolve(out: &mut Vec<Reading>) {
    resolve_iommu(out);
    resolve_pci_extras(out);
    let adapters = read_adapters();
    for (index, gpu) in &adapters {
        resolve_adapter(out, *index, gpu);
    }
    resolve_topology(out, &adapters);
}

/// The enumeration `resolve_gpu` makes, index for index. An adapter that failed
/// to read is left out: `resolve_gpu` reports it as `gpu.<n>.<unreadable>`, and
/// it has no fields to publish here.
fn read_adapters() -> Vec<(usize, GpuInfo)> {
    let Ok(monitor) = crate::UnifiedMonitor::new() else {
        return Vec::new();
    };
    monitor
        .gpus()
        .snapshot_all_partial()
        .into_iter()
        .enumerate()
        .filter_map(|(index, read)| read.ok().map(|gpu| (index, gpu)))
        .collect()
}

fn resolve_adapter(out: &mut Vec<Reading>, index: usize, gpu: &GpuInfo) {
    let base = format!("gpu.{index}");
    let s = &gpu.static_info;
    let d = &gpu.dynamic_info;

    str_or_unavailable(
        out,
        format!("{base}.uuid"),
        s.uuid.as_deref(),
        U::Identifier,
        P::Measured,
        UUID_WHY,
    );
    str_or_unavailable(
        out,
        format!("{base}.pci_bus_id"),
        s.pci_bus_id.as_deref(),
        U::Identifier,
        P::Measured,
        PCI_BUS_WHY,
    );
    str_or_unavailable(
        out,
        format!("{base}.vbios_version"),
        s.vbios_version.as_deref(),
        U::Text,
        P::Measured,
        VBIOS_WHY,
    );
    // The Intel reader stores the kernel driver's name (i915, xe) in this
    // field. A name is not a version.
    let driver_version = if s.vendor == crate::gpu::GpuVendor::Intel {
        None
    } else {
        s.driver_version.as_deref()
    };
    str_or_unavailable(
        out,
        format!("{base}.driver_version"),
        driver_version,
        U::Text,
        P::Measured,
        DRIVER_WHY,
    );
    let capability = s
        .compute_capability
        .map(|(major, minor)| format!("{major}.{minor}"));
    str_or_unavailable(
        out,
        format!("{base}.compute_capability"),
        capability.as_deref(),
        U::Text,
        P::Specification,
        CAPABILITY_WHY,
    );
    push_spec_opt(
        out,
        format!("{base}.shader_cores"),
        s.shader_cores.map(|n| serde_json::json!(n)),
        Some(U::Count),
        CORES_WHY,
    );
    push_spec_opt(
        out,
        format!("{base}.l2_cache"),
        s.l2_cache.map(|n| serde_json::json!(n)),
        Some(U::Bytes),
        L2_WHY,
    );
    push_spec_opt(
        out,
        format!("{base}.num_engines"),
        s.num_engines.map(|n| serde_json::json!(n)),
        Some(U::Count),
        ENGINES_WHY,
    );
    // The NVIDIA, AMD and Intel readers set `integrated` from substrings of
    // the product name ("vega", "arc"), which calls a Radeon 780M discrete.
    // Only Apple's is known rather than guessed.
    if s.vendor == crate::gpu::GpuVendor::Apple {
        out.push(Reading::derived(
            format!("{base}.integrated"),
            serde_json::json!(true),
            None,
        ));
    } else {
        out.push(Reading::unavailable(
            format!("{base}.integrated"),
            None,
            "this vendor's reader guesses it from the product name rather than reading it",
        ));
    }

    let mem = &d.memory;
    push_derived(
        out,
        format!("{base}.memory.free"),
        mem.free.map(|b| serde_json::json!(b)),
        Some(U::Bytes),
        FREE_WHY,
    );
    push_derived(
        out,
        format!("{base}.memory.utilization"),
        mem.utilization.map(|p| serde_json::json!(p)),
        Some(U::Percent),
        MEM_UTIL_WHY,
    );

    let clocks = &d.clocks;
    push_opt(
        out,
        format!("{base}.clocks.memory"),
        clocks.memory.map(|v| serde_json::json!(v)),
        Some(U::Megahertz),
        MEM_CLOCK_WHY,
    );
    push_opt(
        out,
        format!("{base}.clocks.memory.max"),
        clocks.memory_max.map(|v| serde_json::json!(v)),
        Some(U::Megahertz),
        MEM_CLOCK_WHY,
    );
    push_opt(
        out,
        format!("{base}.clocks.sm"),
        clocks.sm.map(|v| serde_json::json!(v)),
        Some(U::Megahertz),
        SM_CLOCK_WHY,
    );
    push_opt(
        out,
        format!("{base}.clocks.video"),
        clocks.video.map(|v| serde_json::json!(v)),
        Some(U::Megahertz),
        VIDEO_CLOCK_WHY,
    );

    let power = &d.power;
    push_spec_opt(
        out,
        format!("{base}.power.default_limit"),
        power.default_limit.map(|p| serde_json::json!(p)),
        Some(U::Milliwatts),
        DEFAULT_LIMIT_WHY,
    );
    push_derived(
        out,
        format!("{base}.power.usage_percent"),
        power.usage_percent.map(|p| serde_json::json!(p)),
        Some(U::Percent),
        USAGE_PERCENT_WHY,
    );

    let thermal = &d.thermal;
    push_opt(
        out,
        format!("{base}.thermal.fan_speed"),
        thermal.fan_speed.map(|p| serde_json::json!(p)),
        Some(U::Percent),
        FAN_SPEED_WHY,
    );
    push_opt(
        out,
        format!("{base}.thermal.fan_rpm"),
        thermal.fan_rpm.map(|r| serde_json::json!(r)),
        Some(U::Rpm),
        FAN_RPM_WHY,
    );

    let link = &d.pcie;
    let pcie: [(&str, Option<serde_json::Value>, U, &str); 8] = [
        (
            "current_gen",
            link.current_gen.map(|v| serde_json::json!(v)),
            U::Count,
            PCIE_WHY,
        ),
        (
            "max_gen",
            link.max_gen.map(|v| serde_json::json!(v)),
            U::Count,
            PCIE_WHY,
        ),
        (
            "current_width",
            link.current_width.map(|v| serde_json::json!(v)),
            U::Count,
            PCIE_WHY,
        ),
        (
            "max_width",
            link.max_width.map(|v| serde_json::json!(v)),
            U::Count,
            PCIE_WHY,
        ),
        (
            "current_speed",
            link.current_speed.map(|v| serde_json::json!(v)),
            U::MegatransfersPerSecond,
            PCIE_WHY,
        ),
        (
            "max_speed",
            link.max_speed.map(|v| serde_json::json!(v)),
            U::MegatransfersPerSecond,
            PCIE_WHY,
        ),
        (
            "tx_throughput",
            link.tx_throughput.map(|v| serde_json::json!(v)),
            U::BytesPerSecond,
            PCIE_THROUGHPUT_WHY,
        ),
        (
            "rx_throughput",
            link.rx_throughput.map(|v| serde_json::json!(v)),
            U::BytesPerSecond,
            PCIE_THROUGHPUT_WHY,
        ),
    ];
    for (suffix, value, unit, why) in pcie {
        push_opt(out, format!("{base}.pcie.{suffix}"), value, Some(unit), why);
    }

    let engines = &d.engines;
    let standard: [(&str, Option<u8>); 5] = [
        ("graphics", engines.graphics),
        ("compute", engines.compute),
        ("encoder", engines.encoder),
        ("decoder", engines.decoder),
        ("copy", engines.copy),
    ];
    for (suffix, value) in standard {
        push_opt(
            out,
            format!("{base}.engine.{suffix}"),
            value.map(|v| serde_json::json!(v)),
            Some(U::Percent),
            ENGINE_WHY,
        );
    }
    if engines.vendor_specific.is_empty() {
        out.push(Reading::unavailable(
            format!("{base}.engine.vendor.<none>"),
            None,
            VENDOR_ENGINE_NONE_WHY,
        ));
    }
    // Two engines can sanitise to one segment; the second gets a numeric suffix
    // rather than silently overwriting the first.
    let mut seen: HashSet<String> = HashSet::new();
    for (name, percent) in &engines.vendor_specific {
        let stem = id_segment(name);
        let mut segment = stem.clone();
        let mut n = 2;
        while !seen.insert(segment.clone()) {
            segment = format!("{stem}_{n}");
            n += 1;
        }
        out.push(Reading::measured(
            format!("{base}.engine.vendor.{segment}"),
            serde_json::json!(percent),
            Some(U::Percent),
        ));
    }
}

fn resolve_topology(out: &mut Vec<Reading>, adapters: &[(usize, GpuInfo)]) {
    let monitor = match GpuTopologyMonitor::new() {
        Ok(monitor) => monitor,
        Err(e) => {
            let why = format!("the GPU topology reader did not run: {e}");
            for (index, _) in adapters {
                push_topology_unavailable(out, *index, &why);
            }
            out.push(Reading::unavailable("gpu.link.<none>", None, why));
            return;
        }
    };

    // The topology reader numbers its nodes itself. The join back to `gpu.<n>` is
    // the PCI address, which both readers take from the same bus, normalised so
    // that NVML's eight-digit domain and sysfs's four-digit one compare equal.
    let mut node_for: HashMap<usize, &GpuTopologyNode> = HashMap::new();
    for (index, gpu) in adapters {
        let Some(key) = gpu.static_info.pci_bus_id.as_deref().and_then(bdf_key) else {
            continue;
        };
        if let Some(node) = monitor.gpus().iter().find(|n| bdf_key(&n.bdf) == Some(key)) {
            node_for.insert(*index, node);
        }
    }

    for (index, _) in adapters {
        let base = format!("gpu.{index}");
        match node_for.get(index) {
            Some(node) => {
                push_opt(
                    out,
                    format!("{base}.topology.numa_node"),
                    node.numa_node.map(|n| serde_json::json!(n)),
                    Some(U::Count),
                    TOPOLOGY_NUMA_WHY,
                );
                push_opt(
                    out,
                    format!("{base}.topology.iommu_group"),
                    node.iommu_group.map(|n| serde_json::json!(n)),
                    Some(U::Count),
                    TOPOLOGY_IOMMU_WHY,
                );
                push_opt(
                    out,
                    format!("{base}.topology.driver"),
                    (!node.driver.trim().is_empty()).then(|| serde_json::json!(node.driver)),
                    Some(U::Identifier),
                    TOPOLOGY_DRIVER_WHY,
                );
            }
            None => push_topology_unavailable(out, *index, TOPOLOGY_UNMATCHED_WHY),
        }
    }

    let adapter_for: HashMap<u32, usize> = node_for
        .iter()
        .map(|(index, node)| (node.index, *index))
        .collect();
    let mut named: HashSet<(usize, usize)> = HashSet::new();
    for link in monitor.links() {
        let (Some(&from), Some(&to)) = (
            adapter_for.get(&link.from_gpu),
            adapter_for.get(&link.to_gpu),
        ) else {
            continue;
        };
        if from == to {
            continue;
        }
        // A pair is one link, however the reader listed it.
        let (a, b) = if from < to { (from, to) } else { (to, from) };
        if !named.insert((a, b)) {
            continue;
        }
        let kind = match link.link_type {
            GpuInterconnectType::NvLink => Some("nvlink"),
            GpuInterconnectType::XGmi => Some("xgmi"),
            GpuInterconnectType::PciePeerToPeer => Some("pcie_peer_to_peer"),
            GpuInterconnectType::PcieThroughCpu => Some("pcie_through_cpu"),
            GpuInterconnectType::PcieThroughHostBridge => Some("pcie_through_host_bridge"),
            GpuInterconnectType::SameBoard => Some("same_board"),
            GpuInterconnectType::None | GpuInterconnectType::Unknown => None,
        };
        let type_id = format!("gpu.{a}.link.{b}.type");
        match kind {
            Some(kind) => out.push(Reading::derived(
                type_id,
                serde_json::json!(kind),
                Some(U::Identifier),
            )),
            None => out.push(Reading::unavailable(
                type_id,
                Some(U::Identifier),
                LINK_TYPE_UNCLASSIFIED_WHY,
            )),
        }
        let bandwidth_id = format!("gpu.{a}.link.{b}.bandwidth");
        match link.bandwidth_per_link_gbs {
            // The reader quotes GB/s in decimal, so 1 GB/s is 10^9 bytes per second.
            Some(gbs) => out.push(Reading::derived(
                bandwidth_id,
                serde_json::json!((gbs * 1e9).round() as u64),
                Some(U::BytesPerSecond),
            )),
            None => out.push(Reading::unavailable(
                bandwidth_id,
                Some(U::BytesPerSecond),
                LINK_BANDWIDTH_WHY,
            )),
        }
    }

    if named.is_empty() {
        let why = if adapters.len() < 2 {
            LINK_NONE_FEWER_WHY
        } else if monitor.links().is_empty() {
            LINK_NONE_NO_LINKS_WHY
        } else {
            LINK_NONE_UNMAPPED_WHY
        };
        out.push(Reading::unavailable("gpu.link.<none>", None, why));
    }
}

fn push_topology_unavailable(out: &mut Vec<Reading>, index: usize, why: &str) {
    out.push(Reading::unavailable(
        format!("gpu.{index}.topology.numa_node"),
        Some(U::Count),
        why,
    ));
    out.push(Reading::unavailable(
        format!("gpu.{index}.topology.iommu_group"),
        Some(U::Count),
        why,
    ));
    out.push(Reading::unavailable(
        format!("gpu.{index}.topology.driver"),
        Some(U::Identifier),
        why,
    ));
}

fn resolve_iommu(out: &mut Vec<Reading>) {
    match crate::iommu::IommuMonitor::new() {
        Ok(monitor) => {
            let overview = monitor.overview();
            out.push(Reading::measured(
                "system.iommu.enabled",
                serde_json::json!(overview.enabled),
                None,
            ));
            if overview.enabled {
                let kind = match overview.iommu_type {
                    crate::iommu::IommuType::IntelVtd => Some("intel_vtd"),
                    crate::iommu::IommuType::AmdVi => Some("amd_vi"),
                    crate::iommu::IommuType::ArmSmmu => Some("arm_smmu"),
                    crate::iommu::IommuType::AppleDart => Some("apple_dart"),
                    crate::iommu::IommuType::Unknown => None,
                };
                match kind {
                    Some(kind) => out.push(Reading::measured(
                        "system.iommu.type",
                        serde_json::json!(kind),
                        Some(U::Identifier),
                    )),
                    None => out.push(Reading::unavailable(
                        "system.iommu.type",
                        Some(U::Identifier),
                        IOMMU_TYPE_WHY,
                    )),
                }
            } else {
                out.push(Reading::unavailable(
                    "system.iommu.type",
                    Some(U::Identifier),
                    IOMMU_DISABLED_WHY,
                ));
            }
        }
        Err(e) => {
            let why = format!("the IOMMU overview was not read: {e}");
            out.push(Reading::unavailable(
                "system.iommu.enabled",
                None,
                why.clone(),
            ));
            out.push(Reading::unavailable(
                "system.iommu.type",
                Some(U::Identifier),
                why,
            ));
        }
    }
    out.push(Reading::unavailable(
        "system.iommu.interrupt_remapping",
        None,
        INTERRUPT_REMAP_WHY,
    ));
}

fn resolve_pci_extras(out: &mut Vec<Reading>) {
    // A failed enumeration is already reported as `pci.<none>` by `resolve_pci`.
    let Ok(monitor) = PciDeviceMonitor::new() else {
        return;
    };
    for dev in monitor.devices() {
        let base = format!("pci.{}", id_segment(&dev.address));
        push_opt(
            out,
            format!("{base}.vendor_id"),
            nonempty(&dev.vendor_id),
            Some(U::Identifier),
            PCI_ID_WHY,
        );
        push_opt(
            out,
            format!("{base}.device_id"),
            nonempty(&dev.device_id),
            Some(U::Identifier),
            PCI_ID_WHY,
        );

        push_opt(
            out,
            format!("{base}.subsystem_vendor_id"),
            dev.subsystem_vendor_id
                .as_ref()
                .map(|s| serde_json::json!(s)),
            Some(U::Identifier),
            PCI_SUBSYSTEM_WHY,
        );
        push_opt(
            out,
            format!("{base}.subsystem_device_id"),
            dev.subsystem_device_id
                .as_ref()
                .map(|s| serde_json::json!(s)),
            Some(U::Identifier),
            PCI_SUBSYSTEM_WHY,
        );

        push_opt(
            out,
            format!("{base}.class_code"),
            nonempty(&dev.class_code),
            Some(U::Identifier),
            PCI_CLASS_CODE_WHY,
        );
        push_opt(
            out,
            format!("{base}.revision"),
            nonempty(&dev.revision),
            Some(U::Identifier),
            PCI_REVISION_WHY,
        );
        push_opt(
            out,
            format!("{base}.kernel_module"),
            nonempty(&dev.kernel_module),
            Some(U::Identifier),
            PCI_KERNEL_MODULE_WHY,
        );

        let iommu_why = if cfg!(any(target_os = "windows", target_os = "macos")) {
            PCI_IOMMU_GROUP_OTHER_WHY
        } else {
            PCI_IOMMU_GROUP_LINUX_WHY
        };
        push_opt(
            out,
            format!("{base}.iommu_group"),
            dev.iommu_group.map(|g| serde_json::json!(g)),
            Some(U::Count),
            iommu_why,
        );

        push_opt(
            out,
            format!("{base}.sriov_capable"),
            dev.sriov_capable.map(|b| serde_json::json!(b)),
            None,
            PCI_SRIOV_WHY,
        );
        push_opt(
            out,
            format!("{base}.sriov_vfs"),
            dev.sriov_vfs.map(|n| serde_json::json!(n)),
            Some(U::Count),
            PCI_SRIOV_VFS_WHY,
        );

        // The reader is `None` on Windows. It used to store the PnP device status
        // here, and that was published as a D-state ("OK").
        push_opt(
            out,
            format!("{base}.power_state"),
            dev.power_state.as_ref().map(|s| serde_json::json!(s)),
            Some(U::Identifier),
            PCI_POWER_STATE_WHY,
        );
    }
}

fn nonempty(value: &str) -> Option<serde_json::Value> {
    nonempty_string(value).map(|s| serde_json::json!(s))
}

fn nonempty_string(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// A string reading when the value exists, otherwise unavailable with `why`.
fn str_or_unavailable(
    out: &mut Vec<Reading>,
    id: String,
    value: Option<&str>,
    unit: U,
    provenance: P,
    why: &str,
) {
    match value {
        Some(v) => push_str_as(out, id, v, unit, provenance),
        None => out.push(Reading::unavailable(id, Some(unit), why)),
    }
}

/// A figure computed from other readings. Absent when any input was.
fn push_derived(
    out: &mut Vec<Reading>,
    id: String,
    value: Option<serde_json::Value>,
    unit: Option<U>,
    why: &str,
) {
    match value {
        Some(v) => out.push(Reading::derived(id, v, unit)),
        None => out.push(Reading::unavailable(id, unit, why)),
    }
}

/// Parse a PCI address into (domain, bus, device, function). Accepts the four-digit
/// domain sysfs uses and the eight-digit one NVML uses, and the two-part form with
/// no domain.
fn bdf_key(address: &str) -> Option<(u32, u32, u32, u32)> {
    let parts: Vec<&str> = address.trim().split(':').collect();
    let (domain, bus, devfn) = match parts.as_slice() {
        [domain, bus, devfn] => (
            u32::from_str_radix(domain, 16).ok()?,
            u32::from_str_radix(bus, 16).ok()?,
            *devfn,
        ),
        [bus, devfn] => (0, u32::from_str_radix(bus, 16).ok()?, *devfn),
        _ => return None,
    };
    let (device, function) = devfn.split_once('.')?;
    Some((
        domain,
        bus,
        u32::from_str_radix(device, 16).ok()?,
        u32::from_str_radix(function, 16).ok()?,
    ))
}
