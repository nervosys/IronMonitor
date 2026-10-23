//! Memory module detail (serial, bank, ranks, form factor), topology analysis, EDAC rows and NUMA detail.

#[allow(unused_imports)]
use super::super::resolve::*;
#[allow(unused_imports)]
use super::super::{Domain as D, Entity, EntityKind as K, Provenance as P, Unit as U};

use crate::memory_topology::{DimmInfo, MemoryTopology};

#[allow(clippy::too_many_arguments)]
fn e(
    add: &mut dyn FnMut(Entity),
    id: &str,
    kind: K,
    unit: Option<U>,
    prov: P,
    nullable: bool,
    description: &str,
) {
    add(Entity::new(
        id,
        D::Memory,
        kind,
        unit,
        prov,
        nullable,
        description,
    ));
}

pub(super) fn declare(add: &mut dyn FnMut(Entity)) {
    // ── Module detail (DimmInfo, from SMBIOS) ───────────────────────────────
    e(add, "memory.dimm.{n}.serial_number", K::Identity, Some(U::Identifier), P::Specification, true,
      "Serial number of the memory module, as its SMBIOS memory device record carries it. Absent for an empty slot.");
    e(
        add,
        "memory.dimm.{n}.bank",
        K::Identity,
        Some(U::Text),
        P::Specification,
        true,
        "Bank label of the module's slot, such as BANK 0, from its SMBIOS memory device record.",
    );
    e(add, "memory.dimm.{n}.ranks", K::Measurement, Some(U::Count), P::Specification, true,
      "Number of ranks on the module, from its SMBIOS memory device record. Absent where the record carries no rank count.");
    e(add, "memory.dimm.{n}.form_factor", K::Identity, Some(U::Text), P::Specification, true,
      "Physical form of the module, such as DIMM, SO-DIMM or On-Package, from its SMBIOS memory device record.");

    // ── Topology analysis (MemoryAnalysis) ──────────────────────────────────
    e(add, "memory.topology.total_slots", K::Measurement, Some(U::Count), P::Specification, true,
      "Number of memory slots the board has, empty ones included, where the firmware lists them. Absent where the reader lists installed modules only.");
    add(Entity::new(
        "memory.topology.populated_slots",
        D::Memory,
        K::Measurement,
        Some(U::Count),
        P::Derived,
        true,
        "Number of DIMM slots the reader established as holding a module, counted from the per-slot populated flags. Unavailable where the slot table cannot be read, as on Linux without root.",
    )
    .derived(&["memory.dimm.{n}.populated"]));
    add(Entity::new(
        "memory.topology.channel_count",
        D::Memory,
        K::Measurement,
        Some(U::Count),
        P::Derived,
        true,
        "Number of memory channels inferred from the DIMM locator names. Absent where no locator names a channel.",
    )
    .derived(&["memory.dimm.{n}.locator"]));
    add(Entity::new(
        "memory.topology.dual_channel",
        D::Memory,
        K::Identity,
        None,
        P::Derived,
        true,
        "Whether the populated modules are spread across two channels, inferred from the DIMM locator names. Absent where the channel count is not established.",
    )
    .derived(&["memory.dimm.{n}.locator"]));
    add(Entity::new(
        "memory.topology.matched_speeds",
        D::Memory,
        K::Identity,
        None,
        P::Derived,
        true,
        "Whether every module reports the same speed, compared across the per-slot speeds. Absent where fewer than two speeds were read.",
    )
    .derived(&["memory.dimm.{n}.speed"]));
    add(Entity::new(
        "memory.topology.matched_capacities",
        D::Memory,
        K::Identity,
        None,
        P::Derived,
        true,
        "Whether every module has the same capacity, compared across the per-slot capacities. Absent where fewer than two capacities were read.",
    )
    .derived(&["memory.dimm.{n}.capacity"]));
    e(add, "memory.topology.max_capacity_bytes", K::Limit, Some(U::Bytes), P::Specification, true,
      "Largest capacity the board supports, from the SMBIOS physical memory array record. Absent because no reader queries that record yet.");

    // ── EDAC error counters (EdacMonitor) ───────────────────────────────────
    e(
        add,
        "memory.ecc.controller.<none>",
        K::Diagnostic,
        None,
        P::Unavailable,
        true,
        "Present when no EDAC memory controller was enumerated, carrying why.",
    );
    e(
        add,
        "memory.ecc.controller.{n}.mc_name",
        K::Identity,
        Some(U::Identifier),
        P::Measured,
        true,
        "Name of the memory controller driver, as the kernel's EDAC subsystem lists it.",
    );
    e(add, "memory.ecc.controller.{n}.ce_count", K::Measurement, Some(U::Count), P::Measured, true,
      "Correctable errors this memory controller has counted since its last reset, from the EDAC subsystem.");
    e(add, "memory.ecc.controller.{n}.ue_count", K::Measurement, Some(U::Count), P::Measured, true,
      "Uncorrectable errors this memory controller has counted since its last reset, from the EDAC subsystem.");
    e(add, "memory.ecc.controller.{n}.ce_noinfo_count", K::Measurement, Some(U::Count), P::Measured, true,
      "Correctable errors this controller counted without attributing them to a DIMM, from the EDAC subsystem.");
    e(add, "memory.ecc.controller.{n}.ue_noinfo_count", K::Measurement, Some(U::Count), P::Measured, true,
      "Uncorrectable errors this controller counted without attributing them to a DIMM, from the EDAC subsystem.");
    e(
        add,
        "memory.ecc.controller.{n}.seconds_since_reset",
        K::Measurement,
        Some(U::Seconds),
        P::Measured,
        true,
        "Seconds since this controller's error counters were last reset, from the EDAC subsystem.",
    );
    e(add, "memory.ecc.controller.{n}.csrow.<none>", K::Diagnostic, None, P::Unavailable, true,
      "Present when this memory controller lists no csrow, carrying why. A csrow is a rank group that EDAC reports DIMMs through.");
    e(
        add,
        "memory.ecc.controller.{n}.csrow.{m}.label",
        K::Identity,
        Some(U::Text),
        P::Measured,
        true,
        "Label EDAC gives this csrow's DIMM, such as DIMM_A1 or a platform-specific name.",
    );
    e(
        add,
        "memory.ecc.controller.{n}.csrow.{m}.mem_type",
        K::Identity,
        Some(U::Identifier),
        P::Measured,
        true,
        "Memory type the csrow's DIMM reports to EDAC, such as DDR4 or DDR5.",
    );
    e(
        add,
        "memory.ecc.controller.{n}.csrow.{m}.size",
        K::Limit,
        Some(U::Bytes),
        P::Measured,
        true,
        "Size of the memory this csrow covers, in bytes, as sysfs reports it.",
    );
    e(
        add,
        "memory.ecc.controller.{n}.csrow.{m}.ce_count",
        K::Measurement,
        Some(U::Count),
        P::Measured,
        true,
        "Correctable errors counted against this csrow since its last reset.",
    );
    e(
        add,
        "memory.ecc.controller.{n}.csrow.{m}.ue_count",
        K::Measurement,
        Some(U::Count),
        P::Measured,
        true,
        "Uncorrectable errors counted against this csrow since its last reset.",
    );
    e(
        add,
        "memory.ecc.controller.{n}.csrow.{m}.location",
        K::Identity,
        Some(U::Text),
        P::Measured,
        true,
        "Channel and slot location of this csrow's DIMM, as EDAC reports it.",
    );
    e(
        add,
        "memory.ecc.controller.{n}.csrow.{m}.grain",
        K::Limit,
        Some(U::Bytes),
        P::Measured,
        true,
        "Error resolution of this csrow in bytes: the smallest span an error can be attributed to.",
    );

    // ── NUMA detail (NumaMonitor) ───────────────────────────────────────────
    e(
        add,
        "memory.numa.{n}.memory_free",
        K::Measurement,
        Some(U::Bytes),
        P::Measured,
        true,
        "Free memory on this NUMA node, in bytes, at the time of the reading.",
    );
    e(
        add,
        "memory.numa.{n}.memory_used",
        K::Measurement,
        Some(U::Bytes),
        P::Measured,
        true,
        "Memory in use on this NUMA node, in bytes, at the time of the reading.",
    );
    e(add, "memory.numa.{n}.hugepages_total", K::Limit, Some(U::Count), P::Measured, true,
      "Huge pages configured on this NUMA node. Absent on platforms that do not report huge pages per node.");
    e(add, "memory.numa.{n}.hugepages_free", K::Measurement, Some(U::Count), P::Measured, true,
      "Huge pages on this NUMA node that are not in use. Absent on platforms that do not report huge pages per node.");
    e(
        add,
        "memory.numa.distance.<none>",
        K::Diagnostic,
        None,
        P::Unavailable,
        true,
        "Present when the platform exposed no NUMA distance matrix, carrying why.",
    );
    e(add, "memory.numa.{n}.distance.{m}", K::Measurement, Some(U::Count), P::Specification, true,
      "Relative cost of memory access from NUMA node n to node m, from the firmware distance table. A node's distance to itself is its local figure.");
}

/// A derived reading when its inputs were read, otherwise unavailable with the reason.
fn push_derived_opt(
    out: &mut Vec<Reading>,
    id: &str,
    value: Option<serde_json::Value>,
    unit: Option<U>,
    why: &str,
) {
    match value {
        Some(v) => out.push(Reading::derived(id, v, unit)),
        None => out.push(Reading::unavailable(id, unit, why)),
    }
}

pub(super) fn resolve(out: &mut Vec<Reading>) {
    // One topology monitor for the DIMM detail and the analysis.
    match crate::memory_topology::MemoryTopologyMonitor::new() {
        Ok(monitor) => {
            let topology = monitor.topology();
            resolve_dimm_detail(out, &topology.dimms);
            resolve_analysis(out, topology);
        }
        Err(e) => {
            let why = format!("memory topology unavailable: {e}");
            for id in [
                "memory.topology.total_slots",
                "memory.topology.populated_slots",
                "memory.topology.channel_count",
                "memory.topology.dual_channel",
                "memory.topology.matched_speeds",
                "memory.topology.matched_capacities",
                "memory.topology.max_capacity_bytes",
            ] {
                let unit = match id {
                    "memory.topology.max_capacity_bytes" => Some(U::Bytes),
                    "memory.topology.dual_channel"
                    | "memory.topology.matched_speeds"
                    | "memory.topology.matched_capacities" => None,
                    _ => Some(U::Count),
                };
                out.push(Reading::unavailable(id, unit, why.clone()));
            }
        }
    }

    resolve_ecc(out);
    resolve_numa(out);
}

fn resolve_dimm_detail(out: &mut Vec<Reading>, dimms: &[DimmInfo]) {
    for (i, dimm) in dimms.iter().enumerate() {
        let base = format!("memory.dimm.{i}");
        let empty = dimm.populated == Some(false);

        push_spec_text(out, format!("{base}.bank"), &dimm.bank);
        push_spec_text(
            out,
            format!("{base}.form_factor"),
            &dimm.form_factor.to_string(),
        );

        if empty {
            for suffix in ["serial_number", "ranks"] {
                out.push(Reading::unavailable(
                    format!("{base}.{suffix}"),
                    if suffix == "ranks" {
                        Some(U::Count)
                    } else {
                        Some(U::Identifier)
                    },
                    "this slot is empty",
                ));
            }
        } else {
            push_spec_text(out, format!("{base}.serial_number"), &dimm.serial_number);
            push_spec_opt(
                out,
                format!("{base}.ranks"),
                dimm.ranks.map(|r| serde_json::json!(r)),
                Some(U::Count),
                "the module's SMBIOS record carried no rank count",
            );
        }
    }
}

fn resolve_analysis(out: &mut Vec<Reading>, topology: &MemoryTopology) {
    let a = &topology.analysis;

    push_spec_opt(
        out,
        "memory.topology.total_slots",
        a.total_slots.map(|n| serde_json::json!(n)),
        Some(U::Count),
        "the reader's source lists installed modules only, so the number of slots was not read",
    );

    if topology.dimms.is_empty() {
        out.push(Reading::unavailable(
            "memory.topology.populated_slots",
            Some(U::Count),
            "the topology reader listed no memory slots",
        ));
    } else {
        out.push(Reading::derived(
            "memory.topology.populated_slots",
            serde_json::json!(a.populated_slots),
            Some(U::Count),
        ));
    }

    push_derived_opt(
        out,
        "memory.topology.channel_count",
        a.channel_count.map(|n| serde_json::json!(n)),
        Some(U::Count),
        "no DIMM locator names a memory channel, so the channel count is not established",
    );
    push_derived_opt(
        out,
        "memory.topology.dual_channel",
        a.dual_channel.map(|b| serde_json::json!(b)),
        None,
        "the channel count was not read, so dual-channel operation is not established",
    );
    push_derived_opt(
        out,
        "memory.topology.matched_speeds",
        a.matched_speeds.map(|b| serde_json::json!(b)),
        None,
        "fewer than two DIMM speeds were read, so there is nothing to match",
    );
    push_derived_opt(
        out,
        "memory.topology.matched_capacities",
        a.matched_capacities.map(|b| serde_json::json!(b)),
        None,
        "fewer than two DIMM capacities were read, so there is nothing to match",
    );

    push_spec_opt(
        out,
        "memory.topology.max_capacity_bytes",
        a.max_capacity_bytes.map(|b| serde_json::json!(b)),
        Some(U::Bytes),
        "the board's maximum capacity is in the SMBIOS memory array record, which no reader queries",
    );
}

fn resolve_ecc(out: &mut Vec<Reading>) {
    let monitor = match crate::edac::EdacMonitor::new() {
        Ok(m) => m,
        Err(e) => {
            // Windows and macOS have no EDAC interface, so this is the common path.
            out.push(Reading::unavailable(
                "memory.ecc.controller.<none>",
                None,
                format!("no ECC error reporting interface is available here: {e}"),
            ));
            return;
        }
    };

    let controllers = monitor.controllers();
    if controllers.is_empty() {
        out.push(Reading::unavailable(
            "memory.ecc.controller.<none>",
            None,
            "the EDAC interface exists but lists no memory controller",
        ));
        return;
    }

    for c in controllers {
        let base = format!("memory.ecc.controller.{}", c.index);
        push_id(out, format!("{base}.mc_name"), &c.mc_name);
        push_opt(
            out,
            format!("{base}.ce_count"),
            c.ce_count.map(|v| serde_json::json!(v)),
            Some(U::Count),
            "this controller's correctable-error counter could not be read",
        );
        push_opt(
            out,
            format!("{base}.ue_count"),
            c.ue_count.map(|v| serde_json::json!(v)),
            Some(U::Count),
            "this controller's uncorrectable-error counter could not be read",
        );
        push_opt(
            out,
            format!("{base}.ce_noinfo_count"),
            c.ce_noinfo_count.map(|v| serde_json::json!(v)),
            Some(U::Count),
            "this controller's count of unattributed correctable errors could not be read",
        );
        push_opt(
            out,
            format!("{base}.ue_noinfo_count"),
            c.ue_noinfo_count.map(|v| serde_json::json!(v)),
            Some(U::Count),
            "this controller's count of unattributed uncorrectable errors could not be read",
        );
        push_opt(
            out,
            format!("{base}.seconds_since_reset"),
            c.seconds_since_reset.map(|v| serde_json::json!(v)),
            Some(U::Seconds),
            "the controller did not report how long its error counters have run",
        );

        if c.csrows.is_empty() {
            out.push(Reading::unavailable(
                format!("{base}.csrow.<none>"),
                None,
                "this controller lists no csrow, the rank groups EDAC reports DIMMs through",
            ));
        }
        for r in &c.csrows {
            let rb = format!("{base}.csrow.{}", r.index);
            push_text(out, format!("{rb}.label"), &r.label);
            push_id(out, format!("{rb}.mem_type"), &r.mem_type.to_string());
            push_opt(
                out,
                format!("{rb}.size"),
                r.size_mb
                    .and_then(|mb| mb.checked_mul(1024 * 1024))
                    .map(|v| serde_json::json!(v)),
                Some(U::Bytes),
                "sysfs reported no size for this csrow",
            );
            push_opt(
                out,
                format!("{rb}.ce_count"),
                r.ce_count.map(|v| serde_json::json!(v)),
                Some(U::Count),
                "this csrow's correctable-error counter could not be read",
            );
            push_opt(
                out,
                format!("{rb}.ue_count"),
                r.ue_count.map(|v| serde_json::json!(v)),
                Some(U::Count),
                "this csrow's uncorrectable-error counter could not be read",
            );
            push_text(out, format!("{rb}.location"), &r.location);
            push_opt(
                out,
                format!("{rb}.grain"),
                r.grain.map(|v| serde_json::json!(v)),
                Some(U::Bytes),
                "the grain size of this csrow could not be read",
            );
        }
    }
}

fn resolve_numa(out: &mut Vec<Reading>) {
    // On failure the base resolver has already reported the NUMA rows as unavailable.
    let Ok(monitor) = crate::numa::NumaMonitor::new() else {
        return;
    };

    for node in monitor.nodes() {
        let base = format!("memory.numa.{}", node.id);
        push_opt(
            out,
            format!("{base}.memory_free"),
            node.memory_free_bytes.map(|v| serde_json::json!(v)),
            Some(U::Bytes),
            "the platform reported no free memory figure for this node",
        );
        push_opt(
            out,
            format!("{base}.memory_used"),
            node.memory_used_bytes.map(|v| serde_json::json!(v)),
            Some(U::Bytes),
            "the platform reported no used memory figure for this node",
        );
        push_opt(
            out,
            format!("{base}.hugepages_total"),
            node.hugepages_total.map(|v| serde_json::json!(v)),
            Some(U::Count),
            "this platform does not report huge pages per NUMA node",
        );
        push_opt(
            out,
            format!("{base}.hugepages_free"),
            node.hugepages_free.map(|v| serde_json::json!(v)),
            Some(U::Count),
            "this platform does not report huge pages per NUMA node",
        );
    }

    match monitor.distance_matrix() {
        Some(matrix) => {
            for from in 0..matrix.size {
                for to in 0..matrix.size {
                    push_opt(
                        out,
                        format!("memory.numa.{from}.distance.{to}"),
                        matrix.distance(from, to).map(|d| serde_json::json!(d)),
                        Some(U::Count),
                        "the distance matrix has no entry for this pair of nodes",
                    );
                }
            }
        }
        None => out.push(Reading::unavailable(
            "memory.numa.distance.<none>",
            None,
            "the platform exposed no NUMA distance matrix, so inter-node distances were not read",
        )),
    }
}
