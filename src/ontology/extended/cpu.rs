//! CPU frequency policy, idle states, turbo, cache detail, hybrid core
//! microarchitecture, crypto acceleration, vulnerability mitigations, kernel
//! security settings, operating system detail, virtualization detail and the
//! interconnect socket count.

#[allow(unused_imports)]
use super::super::resolve::*;
#[allow(unused_imports)]
use super::super::{Domain as D, Entity, EntityKind as K, Provenance as P, Unit as U};

use crate::security_mitigations::{MitigationStatus, SecurityModule};

pub(super) fn declare(add: &mut dyn FnMut(Entity)) {
    // ── Per-processor cpufreq ────────────────────────────────────────────────
    //
    // Indexed by the CPU number `resolve_cpu` uses for `cpu.core.{n}`, so a row
    // here and the base reading for the same processor share an index.
    add(Entity::new(
        "cpu.core.<none>",
        D::Cpu,
        K::Diagnostic,
        None,
        P::Unavailable,
        true,
        "Present when the cpufreq reader could not enumerate processors, carrying why.",
    ));
    add(Entity::new(
        "cpu.core.{n}.online",
        D::Cpu,
        K::Measurement,
        None,
        P::Measured,
        true,
        "Whether the kernel has processor n online. A processor the kernel cannot \
         take offline reports online. Linux only; Windows and macOS expose no \
         per-processor online state here.",
    ));
    add(Entity::new(
        "cpu.core.{n}.governor",
        D::Cpu,
        K::Measurement,
        Some(U::Identifier),
        P::Measured,
        true,
        "Active cpufreq governor of processor n, such as performance, powersave or \
         schedutil. Linux only. Windows selects a power plan instead, which is not \
         a cpufreq governor, so it is not published as one.",
    ));
    add(Entity::new(
        "cpu.core.{n}.governor.available",
        D::Cpu,
        K::Measurement,
        None,
        P::Measured,
        true,
        "Cpufreq governors the kernel offers for processor n, as a list. Linux only.",
    ));
    add(Entity::new(
        "cpu.core.{n}.scaling_driver",
        D::Cpu,
        K::Identity,
        Some(U::Identifier),
        P::Measured,
        true,
        "Cpufreq scaling driver in use for processor n, such as intel_pstate, \
         amd-pstate-epp or acpi-cpufreq. Linux only.",
    ));
    add(Entity::new(
        "cpu.core.{n}.frequency.cpuinfo_min",
        D::Cpu,
        K::Limit,
        Some(U::Megahertz),
        P::Specification,
        true,
        "Lowest clock the hardware supports for processor n, as the cpufreq driver \
         reports it. Declared by the hardware and does not change while the machine \
         runs.",
    ));
    add(Entity::new(
        "cpu.core.{n}.frequency.cpuinfo_max",
        D::Cpu,
        K::Limit,
        Some(U::Megahertz),
        P::Specification,
        true,
        "Highest clock the hardware supports for processor n, as the cpufreq driver \
         reports it. This can be the boost ceiling, and it is not the clock now; \
         compare cpu.core.{n}.frequency for that.",
    ));
    add(Entity::new(
        "cpu.core.{n}.frequency.base",
        D::Cpu,
        K::Limit,
        Some(U::Megahertz),
        P::Specification,
        true,
        "Base (non-turbo) clock of processor n. Declared by the hardware or firmware; \
         on Windows it is the nominal clock from the registry.",
    ));
    add(Entity::new(
        "cpu.core.{n}.energy_preference",
        D::Cpu,
        K::Measurement,
        Some(U::Identifier),
        P::Measured,
        true,
        "Energy performance preference set for processor n, such as balance_performance. \
         Linux with an intel_pstate or amd_pstate driver that exposes it only.",
    ));
    add(Entity::new(
        "cpu.core.{n}.energy_preference.available",
        D::Cpu,
        K::Measurement,
        None,
        P::Measured,
        true,
        "Energy performance preferences processor n accepts, as a list. Linux only.",
    ));

    // ── Per-processor idle states (C-states) ─────────────────────────────────
    add(Entity::new(
        "cpu.core.{n}.idle_state.<none>",
        D::Cpu,
        K::Diagnostic,
        None,
        P::Unavailable,
        true,
        "Present when processor n exposes no idle states, carrying why.",
    ));
    add(Entity::new(
        "cpu.core.{n}.idle_state.{m}.name",
        D::Cpu,
        K::Identity,
        Some(U::Identifier),
        P::Measured,
        true,
        "Name of idle state m of processor n, such as POLL, C1 or C6.",
    ));
    add(Entity::new(
        "cpu.core.{n}.idle_state.{m}.description",
        D::Cpu,
        K::Identity,
        Some(U::Text),
        P::Measured,
        true,
        "Description of idle state m of processor n, as the kernel gives it.",
    ));
    add(Entity::new(
        "cpu.core.{n}.idle_state.{m}.latency",
        D::Cpu,
        K::Limit,
        Some(U::Milliseconds),
        P::Specification,
        true,
        "Exit latency of idle state m of processor n, converted from microseconds. \
         Declared by the firmware or the driver's table, not timed here.",
    ));
    add(Entity::new(
        "cpu.core.{n}.idle_state.{m}.residency",
        D::Cpu,
        K::Limit,
        Some(U::Milliseconds),
        P::Specification,
        true,
        "Target residency of idle state m of processor n, converted from microseconds. \
         It is the shortest stay for which entering the state pays off, as the \
         driver's table declares it.",
    ));
    add(Entity::new(
        "cpu.core.{n}.idle_state.{m}.enabled",
        D::Cpu,
        K::Measurement,
        None,
        P::Measured,
        true,
        "Whether the kernel may enter idle state m of processor n.",
    ));
    add(Entity::new(
        "cpu.core.{n}.idle_state.{m}.usage",
        D::Cpu,
        K::Measurement,
        Some(U::Count),
        P::Measured,
        true,
        "How many times processor n has entered idle state m since boot. A cumulative \
         counter, not a rate; two reads are needed to get one.",
    ));
    add(Entity::new(
        "cpu.core.{n}.idle_state.{m}.time",
        D::Cpu,
        K::Measurement,
        Some(U::Seconds),
        P::Measured,
        true,
        "Total time processor n has spent in idle state m since boot. A cumulative \
         counter, not a rate; two reads are needed to get one.",
    ));

    // ── Turbo ────────────────────────────────────────────────────────────────
    add(Entity::new(
        "cpu.turbo.available",
        D::Cpu,
        K::Identity,
        None,
        P::Measured,
        true,
        "Whether the kernel exposes a control for turbo or boost. Linux only.",
    ));
    add(Entity::new(
        "cpu.turbo.enabled",
        D::Cpu,
        K::Measurement,
        None,
        P::Measured,
        true,
        "Whether turbo or boost is enabled. Linux: the kernel's current state. Windows: \
         boost permitted by the active power scheme (its processor performance boost \
         mode is not Disabled), which is not the same as boost currently active.",
    ));
    add(Entity::new(
        "cpu.turbo.controllable",
        D::Cpu,
        K::Identity,
        None,
        P::Measured,
        true,
        "Whether the kernel offers a boost control that can be switched. Linux only.",
    ));
    add(Entity::new(
        "cpu.turbo.boost_mhz",
        D::Cpu,
        K::Limit,
        Some(U::Megahertz),
        P::Derived,
        true,
        "Clock the turbo ceiling sits above base, in MHz. Not read on any platform: \
         Windows used to subtract its boot-time clock from its rated maximum, which \
         is not a boost figure.",
    )
    .derived(&["cpu.core.{n}.frequency.max", "cpu.core.{n}.frequency.base"]));

    // ── Cache detail ─────────────────────────────────────────────────────────
    add(Entity::new(
        "cpu.cache.<none>",
        D::Cpu,
        K::Diagnostic,
        None,
        P::Unavailable,
        true,
        "Present when no cache instance could be described, carrying why.",
    ));
    add(Entity::new(
        "cpu.cache.{n}.type",
        D::Cpu,
        K::Identity,
        Some(U::Identifier),
        P::Measured,
        true,
        "Kind of cache instance n: data, instruction or unified.",
    ));
    add(Entity::new(
        "cpu.cache.{n}.associativity",
        D::Cpu,
        K::Identity,
        Some(U::Count),
        P::Measured,
        true,
        "Number of ways in cache instance n. Zero means fully associative.",
    ));
    add(Entity::new(
        "cpu.cache.{n}.sets",
        D::Cpu,
        K::Identity,
        Some(U::Count),
        P::Measured,
        true,
        "Number of sets in cache instance n, where the platform publishes it. Linux only.",
    ));
    add(Entity::new(
        "cpu.cache.{n}.partitions",
        D::Cpu,
        K::Identity,
        Some(U::Count),
        P::Measured,
        true,
        "Number of physical partitions in cache instance n, where the platform \
         publishes it. Linux only.",
    ));

    // ── Hybrid core microarchitecture ────────────────────────────────────────
    add(Entity::new(
        "cpu.microarch.p_core_uarch",
        D::Cpu,
        K::Identity,
        Some(U::Identifier),
        P::Specification,
        true,
        "Microarchitecture of the performance cores of a hybrid processor, looked up \
         from its model name. Null for a processor that is not hybrid.",
    ));
    add(Entity::new(
        "cpu.microarch.e_core_uarch",
        D::Cpu,
        K::Identity,
        Some(U::Identifier),
        P::Specification,
        true,
        "Microarchitecture of the efficiency cores of a hybrid processor, looked up \
         from its model name. Null for a processor that is not hybrid.",
    ));

    // ── Crypto acceleration detail ───────────────────────────────────────────
    add(Entity::new(
        "cpu.crypto.feature.{n}.hardware_accelerated",
        D::Cpu,
        K::Identity,
        None,
        P::Specification,
        true,
        "Whether the cryptographic feature at index n runs in hardware. Only \
         accelerated features are indexed, so this is true for every row by \
         construction; a primitive absent from the list runs in software.",
    ));

    // ── CPU vulnerability mitigations ────────────────────────────────────────
    add(Entity::new(
        "cpu.vulnerability.<none>",
        D::Cpu,
        K::Diagnostic,
        None,
        P::Unavailable,
        true,
        "Present when no CPU vulnerability status could be read, carrying why.",
    ));
    add(Entity::new(
        "cpu.vulnerability.{n}.name",
        D::Cpu,
        K::Identity,
        Some(U::Identifier),
        P::Measured,
        true,
        "Kernel name of CPU vulnerability n, such as spectre_v2 or retbleed.",
    ));
    add(Entity::new(
        "cpu.vulnerability.{n}.status",
        D::Cpu,
        K::Measurement,
        Some(U::Identifier),
        P::Measured,
        true,
        "Mitigation state the kernel reports for vulnerability n: not_affected, \
         vulnerable, mitigated or partially_mitigated.",
    ));
    add(Entity::new(
        "cpu.vulnerability.{n}.raw_status",
        D::Cpu,
        K::Measurement,
        Some(U::Text),
        P::Measured,
        true,
        "The kernel's own mitigation text for vulnerability n, verbatim.",
    ));
    add(Entity::new(
        "cpu.vulnerability.{n}.cve",
        D::Cpu,
        K::Identity,
        None,
        P::Specification,
        true,
        "CVE identifiers the reader associates with vulnerability n, as a list. The \
         reader carries this as a fixed table; the kernel does not report it.",
    ));
    add(Entity::new(
        "cpu.vulnerability.{n}.performance_impact",
        D::Cpu,
        K::Measurement,
        None,
        P::Derived,
        true,
        "Whether the kernel's mitigation text names a mitigation with a known \
         performance cost, such as retpoline or IBRS. A parse of that text, not a \
         benchmark.",
    )
    .derived(&["cpu.vulnerability.{n}.raw_status"]));

    // ── Kernel security modules and hardening ────────────────────────────────
    add(Entity::new(
        "system.security.lsm.<none>",
        D::System,
        K::Diagnostic,
        None,
        P::Unavailable,
        true,
        "Present when no recognised Linux security module is listed, carrying why.",
    ));
    add(Entity::new(
        "system.security.lsm.{n}.module",
        D::System,
        K::Identity,
        Some(U::Identifier),
        P::Measured,
        true,
        "Name of security module n, as the kernel's active LSM list gives it.",
    ));
    add(Entity::new(
        "system.security.lsm.{n}.enabled",
        D::System,
        K::Measurement,
        None,
        P::Measured,
        true,
        "Whether security module n is active. Every module the kernel lists is.",
    ));
    add(Entity::new(
        "system.security.lsm.{n}.mode",
        D::System,
        K::Measurement,
        Some(U::Identifier),
        P::Measured,
        true,
        "Mode of security module n. Only SELinux exposes one here, as enforcing or \
         permissive.",
    ));
    add(Entity::new(
        "system.security.aslr_level",
        D::System,
        K::Measurement,
        Some(U::Count),
        P::Measured,
        true,
        "Address space layout randomisation level: 0 off, 1 partial, 2 full.",
    ));
    add(Entity::new(
        "system.security.kptr_restrict",
        D::System,
        K::Measurement,
        None,
        P::Measured,
        true,
        "Whether kernel pointer addresses are hidden from unprivileged users.",
    ));
    add(Entity::new(
        "system.security.dmesg_restrict",
        D::System,
        K::Measurement,
        None,
        P::Measured,
        true,
        "Whether the kernel log is restricted to privileged users.",
    ));
    add(Entity::new(
        "system.security.unprivileged_bpf_disabled",
        D::System,
        K::Measurement,
        None,
        P::Measured,
        true,
        "Whether unprivileged users are barred from loading BPF programs.",
    ));
    add(Entity::new(
        "system.security.ptrace_scope",
        D::System,
        K::Measurement,
        Some(U::Count),
        P::Measured,
        true,
        "Yama ptrace scope: how far one process may trace another. Zero is unrestricted.",
    ));
    add(Entity::new(
        "system.security.lockdown_mode",
        D::System,
        K::Measurement,
        Some(U::Text),
        P::Measured,
        true,
        "Kernel lockdown mode as the kernel reports it, with the active mode in brackets.",
    ));

    // ── Operating system detail ──────────────────────────────────────────────
    add(Entity::new(
        "system.os.family",
        D::System,
        K::Identity,
        Some(U::Identifier),
        P::Measured,
        true,
        "Operating system family IronMonitor identified: linux, windows, macos or freebsd.",
    ));
    add(Entity::new(
        "system.os.is_64bit",
        D::System,
        K::Identity,
        None,
        P::Measured,
        true,
        "Whether the operating system is 64-bit. Read on Windows only; elsewhere the \
         reader takes the width from the IronMonitor binary, so nothing is published.",
    ));
    add(Entity::new(
        "system.kernel.full",
        D::System,
        K::Identity,
        Some(U::Text),
        P::Derived,
        true,
        "Full kernel version line. On Windows it is built from the OS version, since \
         no kernel line is read there.",
    )
    .derived(&["system.kernel.version"]));
    add(Entity::new(
        "system.kernel.cmdline",
        D::System,
        K::Identity,
        Some(U::Text),
        P::Measured,
        true,
        "Kernel command line the machine booted with. Linux only.",
    ));
    add(Entity::new(
        "system.kernel.modules.count",
        D::System,
        K::Measurement,
        Some(U::Count),
        P::Measured,
        true,
        "Number of kernel modules or drivers loaded at the time of the read.",
    ));
    add(Entity::new(
        "system.domain",
        D::System,
        K::Identity,
        Some(U::Text),
        P::Measured,
        true,
        "Network domain name the machine is configured with.",
    ));
    add(Entity::new(
        "system.timezone",
        D::System,
        K::Identity,
        Some(U::Text),
        P::Measured,
        true,
        "Timezone name the operating system is set to, such as America/New_York.",
    ));
    add(Entity::new(
        "system.locale",
        D::System,
        K::Identity,
        Some(U::Text),
        P::Measured,
        true,
        "Locale name the operating system is set to, such as en_US.",
    ));
    add(Entity::new(
        "system.boot.timestamp",
        D::System,
        K::Identity,
        Some(U::Seconds),
        P::Derived,
        true,
        "Unix time, in seconds, of the last boot. On Windows it is computed as the \
         current time minus uptime.",
    )
    .derived(&["system.uptime"]));
    add(Entity::new(
        "system.boot.mode",
        D::System,
        K::Identity,
        Some(U::Identifier),
        P::Measured,
        true,
        "Firmware boot mode: uefi or bios. The macOS reader does not check firmware, \
         so nothing is published there.",
    ));
    add(Entity::new(
        "system.users.count",
        D::System,
        K::Measurement,
        Some(U::Count),
        P::Measured,
        true,
        "Number of users with a login session. Counted on Linux only, from who.",
    ));

    // ── Virtualization detail ────────────────────────────────────────────────
    add(Entity::new(
        "system.virtualization.hypervisor.version",
        D::System,
        K::Identity,
        Some(U::Text),
        P::Measured,
        true,
        "Version of the detected hypervisor. The detector does not read one, so it \
         is never published as a value.",
    ));
    add(Entity::new(
        "system.virtualization.cloud_provider",
        D::System,
        K::Identity,
        Some(U::Text),
        P::Derived,
        true,
        "Cloud provider inferred from the Linux DMI vendor or product strings. A \
         classification of firmware text, not a query to a provider.",
    )
    .derived(&["system.virtualization.platform"]));
    add(Entity::new(
        "system.virtualization.instance_type",
        D::System,
        K::Identity,
        Some(U::Text),
        P::Measured,
        true,
        "Cloud instance type of this machine. The detector does not read one.",
    ));
    add(Entity::new(
        "system.virtualization.ept_npt",
        D::System,
        K::Identity,
        None,
        P::Measured,
        true,
        "Whether the CPU supports second-level address translation, EPT or NPT. The \
         detector does not read it.",
    ));
    add(Entity::new(
        "system.virtualization.iommu",
        D::System,
        K::Identity,
        None,
        P::Measured,
        true,
        "Whether an IOMMU is present. The detector does not read it.",
    ));
    add(Entity::new(
        "system.virtualization.sriov",
        D::System,
        K::Identity,
        None,
        P::Measured,
        true,
        "Whether SR-IOV is available. The detector does not read it.",
    ));
    add(Entity::new(
        "system.virtualization.nested",
        D::System,
        K::Identity,
        None,
        P::Measured,
        true,
        "Whether nested virtualization is available. The detector does not read it.",
    ));
    add(Entity::new(
        "system.virtualization.technology",
        D::System,
        K::Identity,
        Some(U::Text),
        P::Measured,
        true,
        "Hardware virtualisation technology the CPU reports through CPUID: Intel VT-x, \
         AMD-V, or none.",
    ));

    // ── Interconnect ─────────────────────────────────────────────────────────
    add(Entity::new(
        "cpu.interconnect.sockets",
        D::Cpu,
        K::Identity,
        Some(U::Count),
        P::Measured,
        true,
        "Number of processor packages (sockets), counted from the platform's package \
         topology.",
    ));
}

pub(super) fn resolve(out: &mut Vec<Reading>) {
    resolve_cpufreq(out);
    resolve_cache_detail(out);
    resolve_hybrid_cores(out);
    resolve_crypto_acceleration(out);
    resolve_security(out);
    resolve_os_detail(out);
    resolve_virtualization_detail(out);
    resolve_interconnect(out);
}

/// Why a per-processor cpufreq value is absent. Each platform's reader reports a
/// different set of fields, so the reason says which case applies.
fn absent(on_linux: bool, offline: bool, n: u32, what: &str) -> String {
    if !on_linux {
        format!("this platform's cpufreq reader does not report {what}")
    } else if offline {
        format!("processor {n} is offline, so the kernel exposes no cpufreq policy to read the {what} from")
    } else {
        format!("the kernel exposed no {what} for processor {n}")
    }
}

fn idle_absent(on_linux: bool, offline: bool, n: u32) -> String {
    if !on_linux {
        "idle states are read from Linux cpuidle, which this platform's reader does not read"
            .to_string()
    } else if offline {
        format!("processor {n} is offline, so its cpuidle states are not exposed")
    } else {
        format!("the kernel exposed no cpuidle states for processor {n}")
    }
}

fn resolve_cpufreq(out: &mut Vec<Reading>) {
    let on_linux = cfg!(target_os = "linux");
    let monitor = match crate::cpufreq::CpuFreqMonitor::new() {
        Ok(m) => m,
        Err(e) => {
            out.push(Reading::unavailable(
                "cpu.core.<none>",
                None,
                format!("the cpufreq reader could not enumerate processors here: {e}"),
            ));
            push_turbo(
                out,
                None,
                "the cpufreq reader could not run on this machine",
            );
            return;
        }
    };

    if monitor.cpus().is_empty() {
        out.push(Reading::unavailable(
            "cpu.core.<none>",
            None,
            "the cpufreq reader found no processors to describe on this machine",
        ));
    }
    push_turbo(out, Some(*monitor.turbo_status()), "");

    let khz_to_mhz = |khz: u64| serde_json::json!(khz as f64 / 1000.0);

    for cpu in monitor.cpus() {
        let n = cpu.id;
        let offline = cpu.online == Some(false);
        // Linux is the only platform with per-processor policy files, and an
        // offline processor has none to read.
        let live = on_linux && !offline;

        match cpu.online {
            Some(online) => out.push(Reading::measured(
                format!("cpu.core.{n}.online"),
                serde_json::json!(online),
                None,
            )),
            None => out.push(Reading::unavailable(
                format!("cpu.core.{n}.online"),
                None,
                if on_linux {
                    format!(
                        "the kernel has an online file for processor {n} but it could not be read"
                    )
                } else {
                    "this platform's cpufreq reader reports no per-processor online state"
                        .to_string()
                },
            )),
        }

        // A governor the kernel could not name is an absence, not a governor
        // called "unknown".
        let governor = match &cpu.governor {
            crate::cpufreq::Governor::Unknown(name) if name.as_str() == "unknown" => None,
            g => Some(g.to_string()),
        };
        match governor.filter(|_| live) {
            Some(g) => push_id(out, format!("cpu.core.{n}.governor"), &g),
            None => out.push(Reading::unavailable(
                format!("cpu.core.{n}.governor"),
                Some(U::Identifier),
                absent(on_linux, offline, n, "governor"),
            )),
        }

        let governors: Vec<String> = cpu
            .available_governors
            .iter()
            .map(ToString::to_string)
            .collect();
        push_opt(
            out,
            format!("cpu.core.{n}.governor.available"),
            (live && !governors.is_empty()).then(|| serde_json::json!(governors)),
            None,
            &absent(on_linux, offline, n, "governor list"),
        );

        push_opt(
            out,
            format!("cpu.core.{n}.scaling_driver"),
            cpu.scaling_driver
                .as_ref()
                .filter(|_| live)
                .map(|d| serde_json::json!(d)),
            Some(U::Identifier),
            &absent(on_linux, offline, n, "scaling driver"),
        );

        push_spec_opt(
            out,
            format!("cpu.core.{n}.frequency.cpuinfo_min"),
            cpu.cpuinfo_min_freq_khz.map(khz_to_mhz),
            Some(U::Megahertz),
            &absent(on_linux, offline, n, "hardware minimum frequency"),
        );
        push_spec_opt(
            out,
            format!("cpu.core.{n}.frequency.cpuinfo_max"),
            cpu.cpuinfo_max_freq_khz.map(khz_to_mhz),
            Some(U::Megahertz),
            &absent(on_linux, offline, n, "hardware maximum frequency"),
        );
        push_spec_opt(
            out,
            format!("cpu.core.{n}.frequency.base"),
            // Linux only. Off Linux the reader fills this from the registry's
            // `~MHz`, the clock the kernel measured at boot, not a rated base.
            cpu.base_freq_khz.filter(|_| on_linux).map(khz_to_mhz),
            Some(U::Megahertz),
            &absent(on_linux, offline, n, "base frequency"),
        );

        push_opt(
            out,
            format!("cpu.core.{n}.energy_preference"),
            cpu.energy_preference
                .filter(|_| live)
                .map(|e| serde_json::json!(e.to_string())),
            Some(U::Identifier),
            &absent(on_linux, offline, n, "energy performance preference"),
        );
        let preferences: Vec<String> = cpu
            .available_energy_preferences
            .iter()
            .map(ToString::to_string)
            .collect();
        push_opt(
            out,
            format!("cpu.core.{n}.energy_preference.available"),
            (live && !preferences.is_empty()).then(|| serde_json::json!(preferences)),
            None,
            &absent(on_linux, offline, n, "energy preference list"),
        );

        if cpu.idle_states.is_empty() {
            out.push(Reading::unavailable(
                format!("cpu.core.{n}.idle_state.<none>"),
                None,
                idle_absent(on_linux, offline, n),
            ));
        }
        for (m, state) in cpu.idle_states.iter().enumerate() {
            let base = format!("cpu.core.{n}.idle_state.{m}");
            push_id(out, format!("{base}.name"), &state.name);
            push_opt(
                out,
                format!("{base}.description"),
                state.desc.clone().map(|d| serde_json::json!(d)),
                Some(U::Text),
                "the kernel exposes no description for this idle state",
            );
            push_spec_opt(
                out,
                format!("{base}.latency"),
                state
                    .latency_us
                    .map(|us| serde_json::json!(f64::from(us) / 1000.0)),
                Some(U::Milliseconds),
                "the kernel exposes no exit latency for this idle state",
            );
            push_spec_opt(
                out,
                format!("{base}.residency"),
                state
                    .residency_us
                    .map(|us| serde_json::json!(f64::from(us) / 1000.0)),
                Some(U::Milliseconds),
                "the kernel exposes no target residency for this idle state",
            );
            push_opt(
                out,
                format!("{base}.enabled"),
                state.enabled.map(|e| serde_json::json!(e)),
                None,
                "the kernel's disable flag for this idle state could not be read",
            );
            push_opt(
                out,
                format!("{base}.usage"),
                state.usage.map(|u| serde_json::json!(u)),
                Some(U::Count),
                "the kernel exposes no entry count for this idle state",
            );
            push_opt(
                out,
                format!("{base}.time"),
                state
                    .time_us
                    .map(|t| serde_json::json!(t as f64 / 1_000_000.0)),
                Some(U::Seconds),
                "the kernel exposes no time spent in this idle state",
            );
        }
    }
}

fn push_turbo(out: &mut Vec<Reading>, turbo: Option<crate::cpufreq::TurboStatus>, why: &str) {
    let turbo = match turbo {
        Some(t) => t,
        None => {
            for (id, unit) in [
                ("cpu.turbo.available", None),
                ("cpu.turbo.enabled", None),
                ("cpu.turbo.controllable", None),
                ("cpu.turbo.boost_mhz", Some(U::Megahertz)),
            ] {
                out.push(Reading::unavailable(id, unit, why));
            }
            return;
        }
    };

    if cfg!(target_os = "linux") {
        match turbo.available {
            Some(available) => out.push(Reading::measured(
                "cpu.turbo.available",
                serde_json::json!(available),
                None,
            )),
            None => out.push(Reading::unavailable(
                "cpu.turbo.available",
                None,
                "the turbo reader did not reach a verdict on this kernel",
            )),
        }
        if let (Some(true), Some(enabled)) = (turbo.available, turbo.enabled) {
            out.push(Reading::measured(
                "cpu.turbo.enabled",
                serde_json::json!(enabled),
                None,
            ));
        } else if turbo.available == Some(true) {
            // The control exists but its file could not be read. Reporting
            // `false` here would present an unread state as a disabled one.
            out.push(Reading::unavailable(
                "cpu.turbo.enabled",
                None,
                "the kernel exposes a boost control but its state file could not be read",
            ));
        } else {
            out.push(Reading::unavailable(
                "cpu.turbo.enabled",
                None,
                "the kernel exposes no turbo or boost control, so there is no enabled state to read",
            ));
        }
        out.push(Reading::measured(
            "cpu.turbo.controllable",
            serde_json::json!(turbo.controllable),
            None,
        ));
        out.push(Reading::unavailable(
            "cpu.turbo.boost_mhz",
            Some(U::Megahertz),
            "the Linux reader does not compute a boost figure; compare each core's current frequency with its base frequency",
        ));
    } else {
        // Windows infers availability from WMI clock values and has no control
        // interface to read, so neither is reported there. `enabled` is the
        // boost mode the active power scheme permits, read with powercfg. It is
        // not whether boost is running now.
        let why = if cfg!(windows) {
            "the Windows turbo reader reads no boost control and no turbo ceiling"
        } else {
            "this platform's turbo reader does not detect boost state"
        };
        for id in ["cpu.turbo.available", "cpu.turbo.controllable"] {
            out.push(Reading::unavailable(id, None, why));
        }
        match turbo.enabled {
            Some(permitted) => out.push(Reading::measured(
                "cpu.turbo.enabled",
                serde_json::json!(permitted),
                None,
            )),
            None => out.push(Reading::unavailable(
                "cpu.turbo.enabled",
                None,
                if cfg!(windows) {
                    "powercfg did not report the active power scheme's processor performance boost mode"
                } else {
                    "this platform's turbo reader does not detect boost state"
                },
            )),
        }
        match turbo.boost_mhz {
            Some(mhz) => out.push(Reading::derived(
                "cpu.turbo.boost_mhz",
                serde_json::json!(mhz),
                Some(U::Megahertz),
            )),
            None => out.push(Reading::unavailable(
                "cpu.turbo.boost_mhz",
                Some(U::Megahertz),
                "no turbo ceiling is read on this platform: the Windows clocks it would be \
                 derived from are both nominal, so their difference is not a boost figure",
            )),
        }
    }
}

fn resolve_cache_detail(out: &mut Vec<Reading>) {
    let monitor = match crate::cpu_cache::CpuCacheMonitor::new() {
        Ok(m) => m,
        Err(e) => {
            out.push(Reading::unavailable(
                "cpu.cache.<none>",
                None,
                format!("cache topology could not be read here: {e}"),
            ));
            return;
        }
    };

    let caches = monitor.caches();
    if caches.is_empty() {
        out.push(Reading::unavailable(
            "cpu.cache.<none>",
            None,
            "the platform reported no cache instances to describe",
        ));
        return;
    }

    // Same index as `cpu.cache.{i}.level` and the other per-instance rows.
    for (i, cache) in caches.iter().enumerate() {
        let base = format!("cpu.cache.{i}");
        // `Unknown` is what a reader reports when the platform named no type,
        // so it is absent here rather than published as a measured "unknown".
        match cache.cache_type {
            crate::cpu_cache::CacheType::Unknown => out.push(Reading::unavailable(
                format!("{base}.type"),
                Some(U::Identifier),
                "the platform did not say whether this cache is data, instruction or unified",
            )),
            ref other => push_id(
                out,
                format!("{base}.type"),
                &format!("{other:?}").to_lowercase(),
            ),
        }
        push_opt(
            out,
            format!("{base}.associativity"),
            cache.associativity.map(|w| serde_json::json!(w)),
            Some(U::Count),
            "the platform reported no way count for this cache; a fully associative cache is not this case, since Windows reports it as 0xFF, which reads as 0",
        );
        push_opt(
            out,
            format!("{base}.sets"),
            cache.sets.map(|s| serde_json::json!(s)),
            Some(U::Count),
            "the platform does not publish the number of sets for this cache",
        );
        push_opt(
            out,
            format!("{base}.partitions"),
            cache.partitions.map(|p| serde_json::json!(p)),
            Some(U::Count),
            "the platform does not publish the number of partitions for this cache",
        );
    }
}

fn resolve_hybrid_cores(out: &mut Vec<Reading>) {
    let monitor = match crate::cpu_microarch::CpuMicroarchMonitor::new() {
        Ok(m) => m,
        Err(e) => {
            for id in ["cpu.microarch.p_core_uarch", "cpu.microarch.e_core_uarch"] {
                out.push(Reading::unavailable(
                    id,
                    Some(U::Identifier),
                    format!("the processor could not be identified, so its core microarchitectures are unknown: {e}"),
                ));
            }
            return;
        }
    };

    let uarch = &monitor.report().microarch;
    let identified = uarch.hybrid == Some(true) && uarch.name.is_some();
    let why = "this processor is not a hybrid design, or its family was not identified, so no separate core microarchitecture is known";

    // The reader fills an unnamed core with the placeholder "P-core" or "E-core"
    // and an unidentified part with "Unknown ..."; neither is a microarchitecture.
    let usable = |name: &Option<String>| -> Option<serde_json::Value> {
        if !identified {
            return None;
        }
        let s = name.as_deref()?.trim();
        (!s.is_empty() && s != "P-core" && s != "E-core").then(|| serde_json::json!(s))
    };
    push_spec_opt(
        out,
        "cpu.microarch.p_core_uarch",
        usable(&uarch.p_core_uarch),
        Some(U::Identifier),
        why,
    );
    push_spec_opt(
        out,
        "cpu.microarch.e_core_uarch",
        usable(&uarch.e_core_uarch),
        Some(U::Identifier),
        why,
    );
}

fn resolve_crypto_acceleration(out: &mut Vec<Reading>) {
    // The base resolver already reports the `<none>` diagnostic when this
    // monitor cannot run, so a failure here needs no row of its own.
    let monitor = match crate::crypto_accel::CryptoAccelMonitor::new() {
        Ok(m) => m,
        Err(_) => return,
    };
    let accelerated = monitor
        .report()
        .features
        .iter()
        .filter(|f| f.hardware_accelerated)
        .count();
    // Indexed the same way as the base `cpu.crypto.feature.{n}.name` rows.
    for i in 0..accelerated {
        out.push(Reading::spec(
            format!("cpu.crypto.feature.{i}.hardware_accelerated"),
            serde_json::json!(true),
            None,
        ));
    }
}

fn resolve_security(out: &mut Vec<Reading>) {
    const HARDENING: [(&str, Option<U>); 6] = [
        ("system.security.aslr_level", Some(U::Count)),
        ("system.security.kptr_restrict", None),
        ("system.security.dmesg_restrict", None),
        ("system.security.unprivileged_bpf_disabled", None),
        ("system.security.ptrace_scope", Some(U::Count)),
        ("system.security.lockdown_mode", Some(U::Text)),
    ];

    let monitor = match crate::security_mitigations::SecurityMitigationsMonitor::new() {
        Ok(m) => m,
        Err(e) => {
            out.push(Reading::unavailable(
                "cpu.vulnerability.<none>",
                None,
                format!("CPU vulnerability status could not be read here: {e}"),
            ));
            out.push(Reading::unavailable(
                "system.security.lsm.<none>",
                None,
                format!("kernel security modules could not be read here: {e}"),
            ));
            for (id, unit) in HARDENING {
                out.push(Reading::unavailable(
                    id,
                    unit,
                    format!("kernel hardening settings could not be read here: {e}"),
                ));
            }
            return;
        }
    };

    if monitor.vulnerabilities.is_empty() {
        out.push(Reading::unavailable(
            "cpu.vulnerability.<none>",
            None,
            "the kernel exposes no CPU vulnerability entries under /sys/devices/system/cpu/vulnerabilities",
        ));
    }
    for (i, v) in monitor.vulnerabilities.iter().enumerate() {
        let base = format!("cpu.vulnerability.{i}");
        push_id(out, format!("{base}.name"), &v.name);
        let status = match v.status {
            MitigationStatus::NotAffected => Some("not_affected"),
            MitigationStatus::Vulnerable => Some("vulnerable"),
            MitigationStatus::Mitigated => Some("mitigated"),
            MitigationStatus::PartiallyMitigated => Some("partially_mitigated"),
            MitigationStatus::Unknown => None,
        };
        match status {
            Some(s) => push_id(out, format!("{base}.status"), s),
            None => out.push(Reading::unavailable(
                format!("{base}.status"),
                Some(U::Identifier),
                format!(
                    "the kernel reported {:?}, which the reader does not classify as any mitigation state",
                    v.raw_status
                ),
            )),
        }
        push_text(out, format!("{base}.raw_status"), &v.raw_status);
        push_spec_opt(
            out,
            format!("{base}.cve"),
            (!v.cve.is_empty()).then(|| serde_json::json!(v.cve)),
            None,
            "the reader assigns no CVE identifier to this vulnerability",
        );
        out.push(Reading::derived(
            format!("{base}.performance_impact"),
            serde_json::json!(v.performance_impact),
            None,
        ));
    }

    if monitor.security_modules.is_empty() {
        out.push(Reading::unavailable(
            "system.security.lsm.<none>",
            None,
            "the kernel lists no security module this reader recognises in /sys/kernel/security/lsm",
        ));
    }
    for (i, lsm) in monitor.security_modules.iter().enumerate() {
        let base = format!("system.security.lsm.{i}");
        push_id(
            out,
            format!("{base}.module"),
            &format!("{:?}", lsm.module).to_lowercase(),
        );
        out.push(Reading::measured(
            format!("{base}.enabled"),
            serde_json::json!(lsm.enabled),
            None,
        ));
        if lsm.module == SecurityModule::SELinux {
            push_id(out, format!("{base}.mode"), &lsm.mode);
        } else {
            out.push(Reading::unavailable(
                format!("{base}.mode"),
                Some(U::Identifier),
                "the kernel lists this module, but the reader reads a mode only for SELinux",
            ));
        }
    }

    let h = &monitor.hardening;
    push_opt(
        out,
        "system.security.aslr_level",
        h.aslr_level.map(|v| serde_json::json!(v)),
        Some(U::Count),
        "the ASLR state was not read on this platform, or its sysctl could not be read",
    );
    push_opt(
        out,
        "system.security.kptr_restrict",
        h.kptr_restrict.map(|b| serde_json::json!(b)),
        None,
        "the kernel exposes no kptr_restrict setting, which older kernels do not have",
    );
    push_opt(
        out,
        "system.security.dmesg_restrict",
        h.dmesg_restrict.map(|b| serde_json::json!(b)),
        None,
        "the kernel exposes no dmesg_restrict setting, which older kernels do not have",
    );
    push_opt(
        out,
        "system.security.unprivileged_bpf_disabled",
        h.unprivileged_bpf_disabled.map(|b| serde_json::json!(b)),
        None,
        "the kernel exposes no unprivileged_bpf_disabled setting, which older kernels do not have",
    );
    // The reader leaves the scope unread when Yama is absent, so a missing
    // module is not presented as an unrestricted one.
    push_opt(
        out,
        "system.security.ptrace_scope",
        h.ptrace_scope.map(|v| serde_json::json!(v)),
        Some(U::Count),
        "the Yama module is not loaded, so there is no ptrace scope to read",
    );
    // `None` when the lockdown file is absent or brackets no mode. It used to
    // be published as "none", which is a claim about the kernel, not a read.
    push_opt(
        out,
        "system.security.lockdown_mode",
        h.lockdown_mode.as_deref().map(|m| serde_json::json!(m)),
        Some(U::Text),
        "the kernel does not report a lockdown mode here",
    );
}

fn resolve_os_detail(out: &mut Vec<Reading>) {
    use crate::os_info::{BootMode, OsFamily};

    const IDS: [(&str, Option<U>); 11] = [
        ("system.os.family", Some(U::Identifier)),
        ("system.os.is_64bit", None),
        ("system.kernel.full", Some(U::Text)),
        ("system.kernel.cmdline", Some(U::Text)),
        ("system.kernel.modules.count", Some(U::Count)),
        ("system.domain", Some(U::Text)),
        ("system.timezone", Some(U::Text)),
        ("system.locale", Some(U::Text)),
        ("system.boot.timestamp", Some(U::Seconds)),
        ("system.boot.mode", Some(U::Identifier)),
        ("system.users.count", Some(U::Count)),
    ];

    let monitor = match crate::os_info::OsInfoMonitor::new() {
        Ok(m) => m,
        Err(e) => {
            for (id, unit) in IDS {
                out.push(Reading::unavailable(
                    id,
                    unit,
                    format!("the operating system reader could not run here: {e}"),
                ));
            }
            return;
        }
    };

    let info = monitor.info();
    let on_linux = cfg!(target_os = "linux");
    let on_windows = cfg!(windows);
    let on_macos = cfg!(target_os = "macos");

    match &info.os_family {
        OsFamily::Unknown => out.push(Reading::unavailable(
            "system.os.family",
            Some(U::Identifier),
            "the reader did not identify the operating system family",
        )),
        family => push_id(
            out,
            "system.os.family",
            &format!("{family:?}").to_lowercase(),
        ),
    }

    if on_windows {
        out.push(Reading::measured(
            "system.os.is_64bit",
            serde_json::json!(info.is_64bit),
            None,
        ));
    } else {
        out.push(Reading::unavailable(
            "system.os.is_64bit",
            None,
            "this reader takes the width from the IronMonitor binary rather than the operating system, so nothing is published",
        ));
    }

    if info.kernel_full.trim().is_empty() {
        out.push(Reading::unavailable(
            "system.kernel.full",
            Some(U::Text),
            "the platform reported no full kernel version line",
        ));
    } else if on_windows {
        out.push(Reading::derived(
            "system.kernel.full",
            serde_json::json!(info.kernel_full),
            Some(U::Text),
        ));
    } else {
        push_text(out, "system.kernel.full", &info.kernel_full);
    }

    if on_linux {
        push_text(out, "system.kernel.cmdline", &info.kernel_cmdline);
    } else {
        out.push(Reading::unavailable(
            "system.kernel.cmdline",
            Some(U::Text),
            "the kernel command line is read on Linux only",
        ));
    }

    if info.loaded_modules.is_empty() {
        out.push(Reading::unavailable(
            "system.kernel.modules.count",
            Some(U::Count),
            "no loaded module list was read on this platform, or the list was empty",
        ));
    } else {
        out.push(Reading::measured(
            "system.kernel.modules.count",
            serde_json::json!(info.loaded_modules.len()),
            Some(U::Count),
        ));
    }

    push_opt(
        out,
        "system.domain",
        (!info.domain.trim().is_empty()).then(|| serde_json::json!(info.domain.trim())),
        Some(U::Text),
        if on_linux {
            "the kernel reports no NIS domain name, which it shows as (none)"
        } else {
            "this platform's reader does not read a domain name"
        },
    );
    push_opt(
        out,
        "system.timezone",
        (!info.timezone.trim().is_empty()).then(|| serde_json::json!(info.timezone.trim())),
        Some(U::Text),
        "the platform reported no timezone name",
    );
    push_opt(
        out,
        "system.locale",
        (!info.locale.trim().is_empty()).then(|| serde_json::json!(info.locale.trim())),
        Some(U::Text),
        "the platform reported no locale name",
    );

    match info.boot_timestamp {
        None => out.push(Reading::unavailable(
            "system.boot.timestamp",
            Some(U::Seconds),
            "the platform reported no boot time",
        )),
        Some(ts) if on_windows => out.push(Reading::derived(
            "system.boot.timestamp",
            serde_json::json!(ts),
            Some(U::Seconds),
        )),
        Some(ts) => out.push(Reading::measured(
            "system.boot.timestamp",
            serde_json::json!(ts),
            Some(U::Seconds),
        )),
    }

    if on_macos {
        out.push(Reading::unavailable(
            "system.boot.mode",
            Some(U::Identifier),
            "the macOS reader does not read the boot mode: Apple silicon boots through iBoot rather than UEFI, and Intel Macs use EFI",
        ));
    } else {
        match &info.boot_mode {
            BootMode::UEFI => push_id(out, "system.boot.mode", "uefi"),
            BootMode::BIOS => push_id(out, "system.boot.mode", "bios"),
            BootMode::Unknown => out.push(Reading::unavailable(
                "system.boot.mode",
                Some(U::Identifier),
                "the firmware type could not be determined on this platform",
            )),
        }
    }

    match info.user_count {
        Some(n) => out.push(Reading::measured(
            "system.users.count",
            serde_json::json!(n),
            Some(U::Count),
        )),
        None if on_linux => out.push(Reading::unavailable(
            "system.users.count",
            Some(U::Count),
            "the who command could not be run, so no session count was read",
        )),
        None => out.push(Reading::unavailable(
            "system.users.count",
            Some(U::Count),
            "user counting is implemented on Linux only, and this platform's reader leaves the count unset",
        )),
    }
}

fn resolve_virtualization_detail(out: &mut Vec<Reading>) {
    const IDS: [(&str, Option<U>); 8] = [
        ("system.virtualization.hypervisor.version", Some(U::Text)),
        ("system.virtualization.cloud_provider", Some(U::Text)),
        ("system.virtualization.instance_type", Some(U::Text)),
        ("system.virtualization.ept_npt", None),
        ("system.virtualization.iommu", None),
        ("system.virtualization.sriov", None),
        ("system.virtualization.nested", None),
        ("system.virtualization.technology", Some(U::Text)),
    ];

    let monitor = match crate::virtualization::VirtMonitor::new() {
        Ok(m) => m,
        Err(e) => {
            for (id, unit) in IDS {
                out.push(Reading::unavailable(
                    id,
                    unit,
                    format!("virtualization detection could not run here: {e}"),
                ));
            }
            return;
        }
    };

    let hypervisor = monitor.hypervisor();
    let caps = monitor.cpu_capabilities();

    out.push(Reading::unavailable(
        "system.virtualization.hypervisor.version",
        Some(U::Text),
        "the hypervisor detector does not read a version string for any hypervisor it recognises",
    ));

    match hypervisor.as_ref().and_then(|h| h.cloud_provider.clone()) {
        Some(provider) => out.push(Reading::derived(
            "system.virtualization.cloud_provider",
            serde_json::json!(provider),
            Some(U::Text),
        )),
        None => out.push(Reading::unavailable(
            "system.virtualization.cloud_provider",
            Some(U::Text),
            if cfg!(target_os = "linux") {
                "no known cloud vendor appears in the DMI vendor or product strings, which does not show the machine is outside a cloud"
            } else {
                "the cloud vendor check reads Linux DMI strings only"
            },
        )),
    }

    out.push(Reading::unavailable(
        "system.virtualization.instance_type",
        Some(U::Text),
        "the virtualization detector does not read a cloud instance type on any platform",
    ));

    push_opt(
        out,
        "system.virtualization.ept_npt",
        caps.as_ref().and_then(|c| c.ept_npt).map(|b| serde_json::json!(b)),
        None,
        "the detector does not read second-level address translation support; Intel reports EPT in a VMX MSR that needs ring 0",
    );
    push_opt(
        out,
        "system.virtualization.iommu",
        caps.as_ref()
            .and_then(|c| c.iommu)
            .map(|b| serde_json::json!(b)),
        None,
        "the detector does not read IOMMU presence",
    );
    push_opt(
        out,
        "system.virtualization.sriov",
        caps.as_ref()
            .and_then(|c| c.sriov)
            .map(|b| serde_json::json!(b)),
        None,
        "the detector does not read SR-IOV availability",
    );
    push_opt(
        out,
        "system.virtualization.nested",
        caps.as_ref()
            .and_then(|c| c.nested)
            .map(|b| serde_json::json!(b)),
        None,
        "the detector does not read nested virtualization support",
    );

    // The same masking rule as `system.virtualization.hardware_support`: under
    // Hyper-V the root partition's CPUID hides the virtualization bits.
    let is_hyperv = matches!(
        hypervisor.as_ref().map(|h| &h.hypervisor),
        Some(crate::virtualization::Hypervisor::HyperV)
    );
    match caps.as_ref() {
        None => out.push(Reading::unavailable(
            "system.virtualization.technology",
            Some(U::Text),
            "the platform did not report CPU virtualization capabilities",
        )),
        Some(_) if is_hyperv => out.push(Reading::unavailable(
            "system.virtualization.technology",
            Some(U::Text),
            "a Hyper-V hypervisor masks the CPU virtualization bits from CPUID, so the technology cannot be read from this view",
        )),
        Some(c) => push_text(out, "system.virtualization.technology", &c.technology),
    }
}

fn resolve_interconnect(out: &mut Vec<Reading>) {
    match crate::interconnect::InterconnectMonitor::new() {
        Ok(monitor) => push_opt(
            out,
            "cpu.interconnect.sockets",
            monitor
                .topology()
                .sockets
                .filter(|&s| s > 0)
                .map(|s| serde_json::json!(s)),
            Some(U::Count),
            "the platform reported no socket count for this machine",
        ),
        Err(e) => out.push(Reading::unavailable(
            "cpu.interconnect.sockets",
            Some(U::Count),
            format!("the interconnect reader could not run here: {e}"),
        )),
    }
}
