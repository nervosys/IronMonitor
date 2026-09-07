//! Resolving ontology ids to live values.
//!
//! [`super::Ontology`] names what simon *can* report; this module answers what it
//! *does* report, right now, on this machine. Without it the ontology is a glossary:
//! an agent can learn that `gpu.0.thermal.temperature` exists but has no way to read
//! it.
//!
//! ## Declared vs. observed provenance
//!
//! An [`super::Entity`] declares the provenance a value is *supposed* to have. A
//! [`Reading`] carries the provenance it *actually* had when taken. These differ
//! routinely and the difference is the useful part: `gpu.0.thermal.temperature` is
//! declared `measured`, but on a machine whose driver exposes no sensor the reading
//! comes back `unavailable` with a note saying so.
//!
//! The rule this module exists to enforce is that a failed read is never dressed as
//! a successful one. There is no path here that substitutes zero, a previous sample,
//! or a plausible constant for a value that could not be obtained — every such
//! substitution this repository has shipped was indistinguishable from a real
//! reading at the point of consumption, which is precisely what a consuming agent
//! cannot afford.

use serde::{Deserialize, Serialize};

use super::{Entity, Ontology, Provenance, Unit};

/// One resolved value, with the provenance it actually had.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reading {
    /// Concrete id — `gpu.0.name`, never the `gpu.{n}.name` template.
    pub id: String,
    /// `None` whenever `provenance` is [`Provenance::Unavailable`]. A consumer must
    /// render this as unknown; it is never a zero.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
    /// What this reading actually is — not what the entity declares it should be.
    pub provenance: Provenance,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<Unit>,
    /// Why a value is missing, or what qualifies it. Present whenever the observed
    /// provenance differs from the declared one, so the difference is never silent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl Reading {
    fn measured(id: impl Into<String>, value: serde_json::Value, unit: Option<Unit>) -> Self {
        Self {
            id: id.into(),
            value: Some(value),
            provenance: Provenance::Measured,
            unit,
            note: None,
        }
    }

    fn derived(id: impl Into<String>, value: serde_json::Value, unit: Option<Unit>) -> Self {
        Self {
            id: id.into(),
            value: Some(value),
            provenance: Provenance::Derived,
            unit,
            note: None,
        }
    }

    /// A value the hardware or firmware declares about itself rather than one
    /// simon sampled. A DIMM part number and a CPU base clock are both this: a
    /// consumer may rely on them to be stable, and may not treat them as
    /// evidence of the machine's state right now.
    fn spec(id: impl Into<String>, value: serde_json::Value, unit: Option<Unit>) -> Self {
        Self {
            id: id.into(),
            value: Some(value),
            provenance: Provenance::Specification,
            unit,
            note: None,
        }
    }

    /// A value that could not be obtained. The reason is mandatory: "unavailable"
    /// without a cause is the same dead end as a fabricated number.
    fn unavailable(id: impl Into<String>, unit: Option<Unit>, why: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            value: None,
            provenance: Provenance::Unavailable,
            unit,
            note: Some(why.into()),
        }
    }

    /// Whether this reading may be treated as a live observation of the hardware.
    pub fn is_observation(&self) -> bool {
        self.provenance.is_observation() && self.value.is_some()
    }
}

/// Everything simon can currently resolve, in id order.
///
/// Entities with no resolver bound are reported as [`Provenance::Unavailable`] with
/// a note saying so, rather than omitted. An agent comparing this against
/// [`Ontology::build`] can therefore tell "this machine has no such device" apart
/// from "simon cannot read this yet" — two very different facts that an omission
/// would collapse into one.
pub fn snapshot() -> Vec<Reading> {
    let ontology = Ontology::build();
    let mut out = Vec::new();

    resolve_cpu(&mut out);
    resolve_memory(&mut out);
    resolve_system(&mut out);
    resolve_board(&mut out);
    resolve_gpu(&mut out);
    resolve_disk(&mut out);
    resolve_network(&mut out);
    resolve_process(&mut out);
    resolve_thermal(&mut out);
    resolve_power(&mut out);
    resolve_rapl(&mut out);
    resolve_sensors(&mut out);
    resolve_displays(&mut out);
    resolve_virtualization(&mut out);
    resolve_numa(&mut out);
    resolve_ecc(&mut out);
    resolve_pci(&mut out);
    resolve_usb(&mut out);
    resolve_memory_bandwidth(&mut out);
    resolve_microarch(&mut out);
    resolve_crypto(&mut out);
    resolve_input(&mut out);
    resolve_audio(&mut out);
    resolve_cameras(&mut out);
    resolve_printers(&mut out);
    resolve_services(&mut out);
    resolve_kernel_params(&mut out);
    resolve_secure_boot(&mut out);
    resolve_boot_duration(&mut out);
    resolve_bluetooth(&mut out);
    resolve_storage_controllers(&mut out);
    resolve_power_profiles(&mut out);
    resolve_codecs(&mut out);

    // Anything the ontology names but nothing above produced.
    let produced: std::collections::HashSet<&str> = out.iter().map(|r| r.id.as_str()).collect();
    let mut unbound: Vec<Reading> = ontology
        .entities
        .values()
        .filter(|e| {
            // Diagnostics are claims about the reading process, and a resolver emits
            // one only when its condition actually holds. Sweeping them in here
            // asserted the opposite of the truth: `network.<none>` appeared saying
            // "no resolver bound" in a snapshot where the network resolver had just
            // enumerated forty-five readings.
            e.kind != super::EntityKind::Diagnostic
                && !Ontology::is_template(&e.id)
                && !produced.contains(e.id.as_str())
        })
        .map(|e| {
            Reading::unavailable(
                e.id.clone(),
                e.unit,
                "no resolver bound on this build — the entity is defined but simon \
                 does not yet read it here",
            )
        })
        .collect();
    out.append(&mut unbound);

    // Templated entities need the same treatment, and used to be skipped entirely on
    // the grounds that an unexpanded `disk.{n}.model` is not a fact about a machine.
    // That reasoning was wrong in a way this module exists to prevent: a domain whose
    // entities are *all* templates simply vanished from the snapshot, so an agent
    // could not tell "this machine has no disks" from "simon does not read disks".
    // Silence is the one answer a resolver must never give. A domain that produced
    // nothing gets one row saying so, keyed on the template so the id stays traceable
    // back to the schema.
    let mut missing_domains: Vec<Reading> = Vec::new();
    for domain in super::Domain::ALL {
        let prefix = format!("{}.", domain.as_str());
        if out.iter().any(|r| r.id.starts_with(&prefix)) {
            continue;
        }
        let declares_any = ontology.entities.values().any(|e| e.domain == *domain);
        if declares_any {
            missing_domains.push(Reading::unavailable(
                format!("{}.<none>", domain.as_str()),
                None,
                "no instance of this domain was found and no resolver reported why; \
                 the ontology declares entities here but nothing was enumerated",
            ));
        }
    }
    out.append(&mut missing_domains);

    // Last gate: a resolver must not hand an agent a number its own schema calls
    // impossible. Anything outside the declared range becomes unavailable, with the
    // offending value quoted in the note — surfaced, not swallowed, and never
    // clamped. Clamping would turn a 103% core into a plausible 100% and destroy the
    // evidence that the sampling was wrong.
    //
    // This exists because the derivation above got it wrong under load and shipped
    // an impossible percentage. The specific bug is fixed; the gate means the next
    // one degrades to "unknown, and here is why" instead of to a confident lie.
    for reading in out.iter_mut() {
        let Some(value) = reading.value.as_ref().and_then(|v| v.as_f64()) else {
            continue;
        };
        let Some(entity) = lookup_template(&ontology, &reading.id) else {
            continue;
        };
        if let Some(problem) = entity.validate_range(value) {
            reading.value = None;
            reading.provenance = Provenance::Unavailable;
            reading.note = Some(format!(
                "reader produced a value the ontology rejects, so it is withheld \
                 rather than reported: {problem}"
            ));
        }
    }

    // Same shape of gate, for absences dressed as text. A reader that returns
    // "unknown" or "n/a" has told you it does not know, and passing that through
    // as `measured` hands an agent a reading that looks like an answer.
    //
    // This is a final pass rather than only a guard in the push helpers because
    // guarding the helpers did not work. `push_text` and `push_id` were fixed
    // first, on the stated reasoning that they were the only route a text value
    // could take. `push_opt` was a third route, and `pci.*.link.speed` came
    // through it as "unknown" on the next CI run. Four instances of this class
    // have now been found in four different places, which is the argument for
    // catching it where every reading is guaranteed to pass rather than where it
    // happens to be produced.
    for reading in out.iter_mut() {
        if reading.provenance == Provenance::Unavailable {
            continue;
        }
        let Some(text) = reading.value.as_ref().and_then(|v| v.as_str()) else {
            continue;
        };
        if !names_an_absence(text) {
            continue;
        }
        let quoted = text.trim().to_string();
        reading.value = None;
        reading.provenance = Provenance::Unavailable;
        reading.note = Some(format!(
            "reader returned {quoted:?}, which names an absence rather than a value, so it is reported as unavailable rather than as a reading"
        ));
    }

    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// Resolve one id. Templates are rejected: `gpu.{n}.name` is not a question that has
/// an answer, `gpu.0.name` is.
pub fn get(id: &str) -> Option<Reading> {
    if Ontology::is_template(id) {
        return None;
    }
    snapshot().into_iter().find(|r| r.id == id)
}

/// Entities the ontology declares that this build can actually resolve.
///
/// Coverage is a fact about simon, not about the machine, so it is worth being able
/// to ask for directly rather than inferring from a snapshot full of nulls.
pub fn coverage() -> Coverage {
    coverage_of(&snapshot())
}

/// Why no current per-core clock is reported.
///
/// Named per platform because the causes differ, and a caller deciding whether to
/// look elsewhere needs to know which one it hit. On Windows this is not a
/// missing reader: `CallNtPowerInformation` answers, and its answer is the
/// nominal clock rather than the current one — see `get_cpu_frequency`.
#[cfg(target_os = "windows")]
const FREQUENCY_ABSENT: &str = concat!(
    "Windows reports the nominal clock through CallNtPowerInformation, not the ",
    "current one — it returned the maximum unchanged for every core, so there is ",
    "no per-core reading here to report. The real figure needs the ",
    "'% Processor Performance' counter, which simon does not read yet",
);

#[cfg(not(target_os = "windows"))]
const FREQUENCY_ABSENT: &str = "no per-core clock reported";

/// Coverage of an existing snapshot.
///
/// Split out because a caller wanting both the readings and their tally must not
/// take two snapshots to get them: instance counts move between calls on a live
/// machine — a USB device is unplugged, a process exits — so two snapshots
/// legitimately differ in length. A test asserting they match was passing only
/// while the box was quiet, and failed the moment the suite got busier.
pub fn coverage_of(readings: &[Reading]) -> Coverage {
    let bound = readings.iter().filter(|r| r.value.is_some()).count();
    Coverage {
        total: readings.len(),
        resolved: bound,
        unavailable: readings.len() - bound,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Coverage {
    pub total: usize,
    pub resolved: usize,
    pub unavailable: usize,
}

// ── Per-domain resolvers ─────────────────────────────────────────────────────
//
// Each pushes what it can read and an `unavailable` with a cause for what it
// cannot. None of them invent a value on failure.

fn resolve_cpu(out: &mut Vec<Reading>) {
    // Cache topology is independent of the sampling below and must still resolve
    // when the CPU reader fails, so it runs before the early return.
    resolve_cpu_cache(out);

    let stats = read_cpu_stats();

    let stats = match stats {
        Ok(s) => s,
        Err(why) => {
            out.push(Reading::unavailable(
                "cpu.total.utilization",
                Some(Unit::Percent),
                format!("the platform CPU reader failed: {why}"),
            ));
            return;
        }
    };

    out.push(Reading::derived(
        "cpu.total.utilization",
        serde_json::json!(100.0 - stats.total.idle),
        Some(Unit::Percent),
    ));
    out.push(Reading::measured(
        "cpu.total.idle",
        serde_json::json!(stats.total.idle),
        Some(Unit::Percent),
    ));
    out.push(Reading::measured(
        "cpu.cores.logical",
        serde_json::json!(stats.cores.len()),
        Some(Unit::Count),
    ));

    // Declared since the ontology was written and never resolved — the CPU stats
    // reader counts logical processors only, so nothing filled it and the entity
    // fell through to the unbound sweep. Found by
    // `tests/ontology_conformance.rs::non_nullable_entities_are_never_null`, which
    // is the whole argument for deriving tests from the schema: a declared,
    // non-nullable id that no resolver touched is invisible to a hand-written suite.
    match crate::cpu_microarch::CpuMicroarchMonitor::new() {
        Ok(m) if m.report().physical_cores > 0 => out.push(Reading::measured(
            "cpu.cores.physical",
            serde_json::json!(m.report().physical_cores),
            Some(Unit::Count),
        )),
        Ok(_) => out.push(Reading::unavailable(
            "cpu.cores.physical",
            Some(Unit::Count),
            "the platform reported no physical core count distinct from the \
             logical one",
        )),
        Err(e) => out.push(Reading::unavailable(
            "cpu.cores.physical",
            Some(Unit::Count),
            format!("microarchitecture reader unavailable: {e}"),
        )),
    }

    match stats.cores.first().map(|c| c.model.as_str()) {
        // An empty model string is a failed read, not a CPU without a name.
        Some(model) if !model.trim().is_empty() => push_text(out, "cpu.model", model),
        _ => out.push(Reading::unavailable(
            "cpu.model",
            Some(Unit::Text),
            "no core reported a model string",
        )),
    }

    for core in &stats.cores {
        let base = format!("cpu.core.{}", core.id);
        // Derived as `100 - idle`, matching how `cpu.total.utilization` is computed.
        // Summing user and system instead let sampling jitter push the result past
        // 100 — the per-core deltas are taken across a boundary and need not add up
        // — which produced an impossible percentage under heavy load. Deriving from
        // idle is bounded by the same value the total already trusts.
        match core.idle {
            Some(idle) => {
                // The measured input is published beside the figure derived from
                // it. It was read either way and discarding it left the
                // derivation naming an input no consumer could fetch.
                out.push(Reading::measured(
                    format!("{base}.idle"),
                    serde_json::json!(idle),
                    Some(Unit::Percent),
                ));
                out.push(Reading::derived(
                    format!("{base}.utilization"),
                    serde_json::json!(100.0 - idle),
                    Some(Unit::Percent),
                ));
            }
            // The platform exposes no per-processor times. Reporting the system
            // average here would look per-core and be one number repeated.
            _ => {
                for suffix in ["idle", "utilization"] {
                    out.push(Reading::unavailable(
                        format!("{base}.{suffix}"),
                        Some(Unit::Percent),
                        "platform exposes no per-processor times; the system-wide \
                         average is not a per-core reading",
                    ));
                }
            }
        }
        // `current` is an `Option` now; the `> 0` guard stays because a reader
        // could still produce a zero and it would mean the same thing.
        match &core.frequency {
            // Windows has no unprivileged API that reports a current clock, so
            // it multiplies the nominal maximum by the kernel's delivered
            // performance figure. That product is per-core and real, and it is
            // not a direct reading -- publishing it as `Measured` would put a
            // specification behind a measurement's provenance, which is the
            // thing the reader below it was rewritten to stop doing.
            Some(f) if f.current.is_some_and(|c| c > 0) && f.current_is_derived => {
                out.push(Reading::derived(
                    format!("{base}.frequency"),
                    serde_json::json!(f.current),
                    Some(Unit::Megahertz),
                ))
            }
            Some(f) if f.current.is_some_and(|c| c > 0) => out.push(Reading::measured(
                format!("{base}.frequency"),
                serde_json::json!(f.current),
                Some(Unit::Megahertz),
            )),
            _ => out.push(Reading::unavailable(
                format!("{base}.frequency"),
                Some(Unit::Megahertz),
                FREQUENCY_ABSENT,
            )),
        }
        // The rated maximum, which the Windows reader multiplies by delivered
        // performance to get the figure above -- so it is published rather than
        // discarded, and `cpu.core.{n}.frequency` names it as its input.
        //
        // `push_spec_opt`, not `push_opt`: this is a rated figure the firmware
        // states, not one simon measured. It was published as `measured` on
        // every row while the entity beside it declared `Specification` -- the
        // only entity in the ontology whose rows over-claimed their own
        // declaration.
        push_spec_opt(
            out,
            format!("{base}.frequency.max"),
            core.frequency
                .as_ref()
                .and_then(|f| f.max)
                .map(|v| serde_json::json!(v)),
            Some(Unit::Megahertz),
            "platform reports no rated maximum for this core",
        );
        // Deliberately not derived from the maximum: a minimum is a distinct
        // property, and dividing the maximum by four produced a plausible number
        // that was not one.
        out.push(Reading::unavailable(
            format!("{base}.frequency.min"),
            Some(Unit::Megahertz),
            "platform exposes no minimum core frequency; it is not inferable from \
             the maximum",
        ));
    }
}

fn resolve_memory(out: &mut Vec<Reading>) {
    // Slot topology is read from SMBIOS and is independent of the usage figures
    // below, so it runs first and still resolves if those fail.
    resolve_memory_dimms(out);

    let stats = match read_memory_stats() {
        Ok(s) => s,
        Err(why) => {
            for (id, unit) in [
                ("memory.total", Unit::Bytes),
                ("memory.used", Unit::Bytes),
                ("memory.utilization", Unit::Percent),
            ] {
                out.push(Reading::unavailable(
                    id,
                    Some(unit),
                    format!("the platform memory reader failed: {why}"),
                ));
            }
            return;
        }
    };

    // Core readers work in KB; the ontology declares bytes.
    let total = stats.ram.total.saturating_mul(1024);
    let used = stats.ram.used.saturating_mul(1024);
    out.push(Reading::measured(
        "memory.total",
        serde_json::json!(total),
        Some(Unit::Bytes),
    ));
    out.push(Reading::measured(
        "memory.used",
        serde_json::json!(used),
        Some(Unit::Bytes),
    ));
    match total {
        0 => out.push(Reading::unavailable(
            "memory.utilization",
            Some(Unit::Percent),
            "total memory reported as zero, so a percentage has no denominator",
        )),
        t => out.push(Reading::derived(
            "memory.utilization",
            serde_json::json!((used as f64 / t as f64) * 100.0),
            Some(Unit::Percent),
        )),
    }

    // Three outcomes, and until 6.0.0 the type could only express two.
    //
    // A platform that did not report swap and a machine with no swap both
    // arrived here as `0`, and both were published as the definite statement
    // "no swap or pagefile configured". Now `None` says the first and `Some(0)`
    // the second, which is the same distinction this module makes everywhere
    // else and could not make for its own swap rows.
    match stats.swap.total {
        None => {
            for id in ["memory.swap.total", "memory.swap.used"] {
                out.push(Reading::unavailable(
                    id,
                    Some(Unit::Bytes),
                    "the platform did not report swap, so whether this machine has any is unknown",
                ));
            }
        }
        Some(0) => {
            for id in ["memory.swap.total", "memory.swap.used"] {
                out.push(Reading::unavailable(
                    id,
                    Some(Unit::Bytes),
                    "no swap or pagefile configured",
                ));
            }
        }
        Some(total_kb) => {
            out.push(Reading::measured(
                "memory.swap.total",
                serde_json::json!(total_kb.saturating_mul(1024)),
                Some(Unit::Bytes),
            ));
            match stats.swap.used {
                Some(used_kb) => out.push(Reading::measured(
                    "memory.swap.used",
                    serde_json::json!(used_kb.saturating_mul(1024)),
                    Some(Unit::Bytes),
                )),
                None => out.push(Reading::unavailable(
                    "memory.swap.used",
                    Some(Unit::Bytes),
                    "the platform reported a swap total and no used figure",
                )),
            }
        }
    }
}

fn resolve_system(out: &mut Vec<Reading>) {
    match crate::os_info::OsInfoMonitor::new() {
        Ok(monitor) => {
            let info = monitor.info();
            push_text(out, "system.os.name", &info.os_name);
            push_id(out, "system.os.build", &info.os_build);
            // Read by `os_info` since long before the ontology named them.
            push_text(out, "system.hostname", &info.hostname);
            push_text(out, "system.os.version", &info.os_version);
            push_text(out, "system.kernel.version", &info.kernel_version);
            push_id(out, "system.architecture", &info.architecture);
            if info.uptime_seconds > 0 {
                out.push(Reading::measured(
                    "system.uptime",
                    serde_json::json!(info.uptime_seconds),
                    Some(Unit::Seconds),
                ));
            } else {
                out.push(Reading::unavailable(
                    "system.uptime",
                    Some(Unit::Seconds),
                    "uptime read as zero, which would mean the machine booted this \
                     instant",
                ));
            }
        }
        Err(e) => {
            for (id, unit) in [
                ("system.os.name", Unit::Text),
                ("system.os.build", Unit::Identifier),
                ("system.uptime", Unit::Seconds),
            ] {
                out.push(Reading::unavailable(
                    id,
                    Some(unit),
                    format!("OS info reader failed: {e}"),
                ));
            }
        }
    }
}

fn resolve_board(out: &mut Vec<Reading>) {
    match detect_board() {
        Some(board) => {
            push_text(out, "board.model", &board.hardware.model);
            match &board.hardware.module {
                Some(m) => push_text(out, "board.manufacturer", m),
                None => out.push(Reading::unavailable(
                    "board.manufacturer",
                    Some(Unit::Text),
                    "SMBIOS reported no baseboard manufacturer",
                )),
            }
        }
        None => {
            for id in ["board.model", "board.manufacturer"] {
                out.push(Reading::unavailable(
                    id,
                    Some(Unit::Text),
                    "no board detection on this platform",
                ));
            }
        }
    }

    resolve_firmware(out);
    resolve_tpm(out);
}

fn resolve_firmware(out: &mut Vec<Reading>) {
    const IDS: [&str; 3] = [
        "board.firmware.vendor",
        "board.firmware.product",
        "board.firmware.boot_mode",
    ];

    let inventory = match crate::firmware::FirmwareInventory::new() {
        Ok(i) => i,
        Err(e) => {
            for id in IDS {
                out.push(Reading::unavailable(
                    id,
                    Some(Unit::Text),
                    format!("firmware inventory unavailable: {e}"),
                ));
            }
            return;
        }
    };

    push_text(out, "board.firmware.vendor", inventory.system_vendor());
    push_text(out, "board.firmware.product", inventory.system_product());
    push_id(
        out,
        "board.firmware.boot_mode",
        &format!("{:?}", inventory.boot_mode()).to_lowercase(),
    );

    for (i, entry) in inventory.items().iter().enumerate() {
        push_id(
            out,
            format!("board.firmware.{i}.component"),
            &format!("{:?}", entry.component).to_lowercase(),
        );
        push_text(out, format!("board.firmware.{i}.version"), &entry.version);
    }
}

fn resolve_tpm(out: &mut Vec<Reading>) {
    const DETAIL_IDS: [&str; 4] = [
        "board.tpm.version",
        "board.tpm.manufacturer",
        "board.tpm.status",
        "board.tpm.measured_boot",
    ];

    let monitor = match crate::tpm::TpmMonitor::new() {
        Ok(m) => m,
        Err(e) => {
            // Not knowing whether a TPM exists is different from knowing there is
            // none, so `present` goes unavailable here rather than false.
            out.push(Reading::unavailable(
                "board.tpm.present",
                None,
                format!("TPM enumeration failed: {e}"),
            ));
            for id in DETAIL_IDS {
                out.push(Reading::unavailable(
                    id,
                    None,
                    format!("TPM enumeration failed: {e}"),
                ));
            }
            return;
        }
    };

    // A successful enumeration that found nothing is a reading: this machine has
    // no TPM. That is exactly the case `present` exists to state.
    out.push(Reading::measured(
        "board.tpm.present",
        serde_json::json!(monitor.has_tpm()),
        None,
    ));

    let Some(tpm) = monitor.tpm() else {
        for id in DETAIL_IDS {
            out.push(Reading::unavailable(
                id,
                None,
                "no TPM present on this machine",
            ));
        }
        return;
    };

    // `Unknown` on either of these is the reader having failed to determine the
    // value, not the device reporting "unknown". Passing it through as a measured
    // identifier would let an agent conclude a TPM's state had been established
    // when it had not — and for a security property that is the wrong way to be
    // wrong. Presence is separately reported above and stays true either way.
    use crate::tpm::{TpmStatus, TpmVersion};
    match tpm.version {
        TpmVersion::Unknown => out.push(Reading::unavailable(
            "board.tpm.version",
            Some(Unit::Identifier),
            "a TPM is present but its specification version could not be determined",
        )),
        v => push_id(out, "board.tpm.version", &format!("{v:?}").to_lowercase()),
    }
    push_text(out, "board.tpm.manufacturer", &tpm.manufacturer);
    match tpm.status {
        TpmStatus::Unknown => out.push(Reading::unavailable(
            "board.tpm.status",
            Some(Unit::Identifier),
            "a TPM is present but whether it is enabled could not be determined",
        )),
        s => push_id(out, "board.tpm.status", &format!("{s:?}").to_lowercase()),
    }
    // `None` here is not `false`. On Windows the positive comes from a Measured
    // Boot log and the negative cannot be evidenced at all; on macOS nothing is
    // consulted. Publishing a measured `false` would tell a security check that
    // integrity measurement is off, which is a claim this crate cannot make.
    push_opt(
        out,
        "board.tpm.measured_boot".to_string(),
        tpm.measured_boot.map(|b| serde_json::json!(b)),
        None,
        concat!(
            "a TPM is present but whether platform integrity measurements ",
            "are active could not be established"
        ),
    );
}

fn resolve_virtualization(out: &mut Vec<Reading>) {
    const IDS: [&str; 4] = [
        "system.virtualization.platform",
        "system.virtualization.hypervisor",
        "system.virtualization.detection_method",
        "system.virtualization.hardware_support",
    ];

    let monitor = match crate::virtualization::VirtMonitor::new() {
        Ok(m) => m,
        Err(e) => {
            for id in IDS {
                out.push(Reading::unavailable(
                    id,
                    None,
                    format!("virtualization detection unavailable: {e}"),
                ));
            }
            return;
        }
    };

    let hypervisor = monitor.hypervisor();

    // Hyper-V used to be the one case simon could not call: with
    // virtualization-based security on — the Windows 11 default — the host OS
    // runs as the Hyper-V *root partition*, so CPUID reports "Microsoft Hv" on a
    // bare-metal workstation exactly as it does inside a guest. 3.7.0 reported
    // this entity as unavailable rather than guess, because it is the entity an
    // agent consults before trusting every other reading.
    //
    // 3.10.0 reads the partition privilege mask from CPUID leaf 0x40000003, which
    // answers it: only a root partition holds CreatePartitions and CpuManagement.
    // The special case below is now just the ordinary determination.
    let is_hyperv = matches!(
        hypervisor.as_ref().map(|h| &h.hypervisor),
        Some(crate::virtualization::Hypervisor::HyperV)
    );

    // Bare metal is a determination, not a failure to detect one.
    let platform = if monitor.is_container() {
        "container"
    } else if monitor.is_virtual_machine() {
        "virtual_machine"
    } else {
        "bare_metal"
    };
    push_id(out, "system.virtualization.platform", platform);

    match hypervisor {
        Some(h) => {
            push_id(
                out,
                "system.virtualization.hypervisor",
                &format!("{:?}", h.hypervisor).to_lowercase(),
            );
            push_text(
                out,
                "system.virtualization.detection_method",
                &h.detection_method,
            );
        }
        None => {
            out.push(Reading::unavailable(
                "system.virtualization.hypervisor",
                Some(Unit::Identifier),
                "no hypervisor detected; this machine appears to be bare metal",
            ));
            out.push(Reading::unavailable(
                "system.virtualization.detection_method",
                Some(Unit::Text),
                "no hypervisor was detected, so no detection method applied",
            ));
        }
    }

    // A running hypervisor masks the virtualization bits from CPUID, so a `false`
    // here under one means "not visible", not "not supported" — reporting it would
    // tell an agent this CPU cannot virtualize while it is actively virtualizing.
    match monitor.cpu_capabilities() {
        Some(c) if !is_hyperv => out.push(Reading::measured(
            "system.virtualization.hardware_support",
            serde_json::json!(c.hardware_virt),
            None,
        )),
        Some(_) => out.push(Reading::unavailable(
            "system.virtualization.hardware_support",
            None,
            "a hypervisor is masking the CPU virtualization bits from CPUID; \
             absence here would not mean the hardware lacks them",
        )),
        None => out.push(Reading::unavailable(
            "system.virtualization.hardware_support",
            None,
            "the platform did not report CPU virtualization capabilities",
        )),
    }
}

fn resolve_numa(out: &mut Vec<Reading>) {
    let monitor = match crate::numa::NumaMonitor::new() {
        Ok(m) => m,
        Err(e) => {
            for id in ["memory.numa.nodes", "memory.numa.is_numa"] {
                out.push(Reading::unavailable(
                    id,
                    None,
                    format!("NUMA topology unavailable: {e}"),
                ));
            }
            return;
        }
    };

    let summary = monitor.summary();
    out.push(Reading::measured(
        "memory.numa.nodes",
        serde_json::json!(summary.node_count),
        Some(Unit::Count),
    ));
    out.push(Reading::measured(
        "memory.numa.is_numa",
        serde_json::json!(summary.is_numa),
        None,
    ));

    for node in monitor.nodes() {
        let base = format!("memory.numa.{}", node.id);
        out.push(Reading::measured(
            format!("{base}.cpus"),
            serde_json::json!(node.cpus.len()),
            Some(Unit::Count),
        ));
        push_opt(
            out,
            format!("{base}.memory"),
            (node.memory_total_bytes > 0).then(|| serde_json::json!(node.memory_total_bytes)),
            Some(Unit::Bytes),
            "the platform reported no memory total for this node",
        );
    }
}

fn resolve_ecc(out: &mut Vec<Reading>) {
    const IDS: [&str; 3] = [
        "memory.ecc.active",
        "memory.ecc.correctable_errors",
        "memory.ecc.uncorrectable_errors",
    ];

    let monitor = match crate::edac::EdacMonitor::new() {
        Ok(m) => m,
        Err(e) => {
            // On Windows and macOS there is no EDAC equivalent simon reads, so
            // this is the common path. The note says so rather than implying the
            // machine has no ECC — which would be a claim about the hardware
            // rather than about the reader.
            for id in IDS {
                out.push(Reading::unavailable(
                    id,
                    None,
                    format!("no ECC error reporting interface available here: {e}"),
                ));
            }
            return;
        }
    };

    let overview = monitor.overview();
    if overview.total_controllers == 0 {
        for id in IDS {
            out.push(Reading::unavailable(
                id,
                None,
                "no memory controller exposed ECC reporting; the platform interface \
                 exists but enumerated nothing",
            ));
        }
        return;
    }

    out.push(Reading::measured(
        "memory.ecc.active",
        serde_json::json!(overview.ecc_active),
        None,
    ));
    out.push(Reading::measured(
        "memory.ecc.correctable_errors",
        serde_json::json!(overview.total_ce),
        Some(Unit::Count),
    ));
    out.push(Reading::measured(
        "memory.ecc.uncorrectable_errors",
        serde_json::json!(overview.total_ue),
        Some(Unit::Count),
    ));
}

fn resolve_pci(out: &mut Vec<Reading>) {
    let monitor = match crate::pci_devices::PciDeviceMonitor::new() {
        Ok(m) => m,
        Err(e) => {
            out.push(Reading::unavailable(
                "pci.<none>",
                None,
                format!("PCI enumeration failed: {e}"),
            ));
            return;
        }
    };
    if monitor.devices().is_empty() {
        out.push(Reading::unavailable(
            "pci.<none>",
            None,
            "no PCI devices enumerated on this machine",
        ));
        return;
    }

    for dev in monitor.devices() {
        let base = format!("pci.{}", id_segment(&dev.address));
        push_text(out, format!("{base}.vendor"), &dev.vendor_name);
        push_text(out, format!("{base}.device"), &dev.device_name);
        push_id(
            out,
            format!("{base}.class"),
            &format!("{:?}", dev.class).to_lowercase(),
        );
        push_opt(
            out,
            format!("{base}.driver"),
            (!dev.driver.trim().is_empty()).then(|| serde_json::json!(dev.driver)),
            Some(Unit::Text),
            "no driver is bound to this device",
        );

        match &dev.link_info {
            Some(link) => {
                for (suffix, value) in [
                    ("link.width", &link.width),
                    ("link.max_width", &link.max_width),
                    ("link.speed", &link.speed),
                    ("link.max_speed", &link.max_speed),
                ] {
                    push_opt(
                        out,
                        format!("{base}.{suffix}"),
                        (!value.trim().is_empty()).then(|| serde_json::json!(value)),
                        Some(Unit::Identifier),
                        "the device did not report this link property",
                    );
                }
            }
            None => {
                for suffix in [
                    "link.width",
                    "link.max_width",
                    "link.speed",
                    "link.max_speed",
                ] {
                    out.push(Reading::unavailable(
                        format!("{base}.{suffix}"),
                        Some(Unit::Identifier),
                        "Windows populates the PCIe link properties on the device \
                         node for endpoints only. Bridges, switch ports and host \
                         bridges carry none, and neither do devices that are not \
                         PCIe at all; this is one of those. Measured on a 64-device \
                         host: 23 carry the properties, and simon reports all 23",
                    ));
                }
            }
        }

        // The reader uses -1 for "no affinity". Passing that through would give an
        // agent a node number that does not exist.
        push_opt(
            out,
            format!("{base}.numa_node"),
            (dev.numa_node >= 0).then(|| serde_json::json!(dev.numa_node)),
            Some(Unit::Count),
            "the platform reports no NUMA affinity for this device",
        );
    }
}

fn resolve_usb(out: &mut Vec<Reading>) {
    let monitor = match crate::usb::UsbMonitor::new() {
        Ok(m) => m,
        Err(e) => {
            out.push(Reading::unavailable(
                "usb.<none>",
                None,
                format!("USB enumeration failed: {e}"),
            ));
            return;
        }
    };
    if monitor.devices().is_empty() {
        out.push(Reading::unavailable(
            "usb.<none>",
            None,
            "no USB devices enumerated on this machine",
        ));
        return;
    }

    for dev in monitor.devices() {
        // The device's own path, not enumeration order and not bus-and-port.
        //
        // This used to be `{bus_number}_{port_number}` with a comment arguing
        // that an index would repoint every id when an unrelated device was
        // unplugged -- and the Windows reader filled those two fields with `0`
        // and the index. Bus and port cannot fix it either: of this machine's
        // 41 USB nodes only 14 have a hub and port Windows reports, and the
        // composite-device interfaces share their parent's location, so the
        // ids would have been stable and collided. `UsbDevice::address` is the
        // platform's own device path, which is unique and moves only with the
        // device.
        let base = format!("usb.{}", id_segment(&dev.address));
        push_opt(
            out,
            format!("{base}.product"),
            dev.product
                .as_ref()
                .or(dev.description.as_ref())
                .map(|p| serde_json::json!(p)),
            Some(Unit::Text),
            "the device publishes no product string",
        );
        push_opt(
            out,
            format!("{base}.manufacturer"),
            dev.manufacturer.as_ref().map(|m| serde_json::json!(m)),
            Some(Unit::Text),
            "the device publishes no manufacturer string",
        );
        // A root hub's PnP id carries no VID_ or PID_ at all, so these were
        // published as the identifier "0000" — a value indistinguishable from a
        // device that reports it.
        push_opt(
            out,
            format!("{base}.vendor_id"),
            dev.vendor_id.map(|v| serde_json::json!(format!("{v:04x}"))),
            Some(Unit::Identifier),
            "this entry carries no USB vendor id; root hubs and virtual devices have none",
        );
        push_opt(
            out,
            format!("{base}.product_id"),
            dev.product_id
                .map(|p| serde_json::json!(format!("{p:04x}"))),
            Some(Unit::Identifier),
            "this entry carries no USB product id; root hubs and virtual devices have none",
        );
        // `Unknown` on either of these is the descriptor not having been read, not
        // a device that identifies itself as unknown. Third occurrence of this
        // shape in one sweep, which is why it is called out in the handoff.
        use crate::usb::{UsbDeviceClass, UsbSpeed};
        match dev.class {
            UsbDeviceClass::Unknown => out.push(Reading::unavailable(
                format!("{base}.class"),
                Some(Unit::Identifier),
                concat!(
                    "no class was recorded for this node: no device-descriptor ",
                    "class, no interface class, and no hub identifier, in ",
                    "either its hardware or its compatible ids. The usual cases ",
                    "are a composite parent, which declares class 00 to say the ",
                    "class is defined per interface and leaves its interfaces to ",
                    "answer, and a node enumerated on another bus -- a USBSTOR ",
                    "disk or a USB4 router -- which has no USB device descriptor ",
                    "to declare one from"
                ),
            )),
            c => push_id(
                out,
                format!("{base}.class"),
                &format!("{c:?}").to_lowercase(),
            ),
        }
        match dev.speed {
            UsbSpeed::Unknown => out.push(Reading::unavailable(
                format!("{base}.speed"),
                Some(Unit::Identifier),
                concat!(
                    "no hub reported a negotiated speed for this device. The ",
                    "speed is a property of a link, so a node that sits on no ",
                    "upstream hub port has none to report -- a root hub is the ",
                    "usual case, and reporting one for it would be describing a ",
                    "link that does not exist. A device on a bus this reader ",
                    "does not walk reaches the same answer by a different route"
                ),
            )),
            s => push_id(
                out,
                format!("{base}.speed"),
                &format!("{s:?}").to_lowercase(),
            ),
        }
    }
}

fn resolve_memory_dimms(out: &mut Vec<Reading>) {
    const PER_SLOT: [&str; 10] = [
        "capacity",
        "speed",
        "configured_speed",
        "type",
        "manufacturer",
        "part_number",
        "ecc",
        "data_width",
        "total_width",
        "voltage",
    ];

    let monitor = match crate::memory_topology::MemoryTopologyMonitor::new() {
        Ok(m) => m,
        Err(e) => {
            out.push(Reading::unavailable(
                "memory.dimm.0.locator",
                Some(Unit::Text),
                format!("memory topology unavailable: {e}"),
            ));
            return;
        }
    };

    for (i, dimm) in monitor.topology().dimms.iter().enumerate() {
        let base = format!("memory.dimm.{i}");
        push_spec_text(out, format!("{base}.locator"), &dimm.locator);
        // Firmware-declared like the rest of the cluster: the board says the
        // slot is filled, and simon did not look inside the case.
        out.push(Reading::spec(
            format!("{base}.populated"),
            serde_json::json!(dimm.populated),
            None,
        ));

        // An empty slot is a real slot with nothing in it. Reporting zeros for its
        // capacity and speed would describe a module of no size running at no
        // speed, which is not what the board is telling us.
        if !dimm.populated {
            for suffix in PER_SLOT {
                out.push(Reading::unavailable(
                    format!("{base}.{suffix}"),
                    None,
                    "this slot is empty",
                ));
            }
            continue;
        }

        push_spec_opt(
            out,
            format!("{base}.capacity"),
            (dimm.capacity_bytes > 0).then(|| serde_json::json!(dimm.capacity_bytes)),
            Some(Unit::Bytes),
            "SMBIOS reported no capacity for a slot it marked populated",
        );
        push_spec_opt(
            out,
            format!("{base}.speed"),
            (dimm.speed_mts > 0).then(|| serde_json::json!(dimm.speed_mts)),
            Some(Unit::MegatransfersPerSecond),
            "SMBIOS reported no rated speed",
        );
        push_spec_opt(
            out,
            format!("{base}.configured_speed"),
            (dimm.configured_speed_mts > 0).then(|| serde_json::json!(dimm.configured_speed_mts)),
            Some(Unit::MegatransfersPerSecond),
            "SMBIOS reported no configured speed",
        );
        push_spec_id(
            out,
            format!("{base}.type"),
            &format!("{:?}", dimm.memory_type).to_lowercase(),
        );
        push_spec_text(out, format!("{base}.manufacturer"), &dimm.manufacturer);
        push_spec_text(out, format!("{base}.part_number"), &dimm.part_number);
        push_spec_opt(
            out,
            format!("{base}.data_width"),
            dimm.data_width_bits
                .filter(|w| *w > 0)
                .map(|w| serde_json::json!(w)),
            Some(Unit::Count),
            "SMBIOS reported no data width",
        );
        push_spec_opt(
            out,
            format!("{base}.total_width"),
            dimm.total_width_bits
                .filter(|w| *w > 0)
                .map(|w| serde_json::json!(w)),
            Some(Unit::Count),
            "SMBIOS reported no total width",
        );
        // ECC is the widths differing, so it can only be stated when both are
        // known. Two zeros are equal, and would otherwise report "no ECC".
        //
        // That guard was right about the danger and wrong about the sentinel:
        // the readers defaulted `data_width` to **64** and `total_width` to
        // whatever `data_width` became, so both passed `> 0` and this published
        // `ecc: false` as `derived` for any DIMM whose widths were not read.
        // A `> 0` guard only defends against a zero; a plausible default walks
        // through it. The fields are `Option` now and the question is asked
        // directly.
        if let Some(ecc) = dimm.is_ecc() {
            out.push(Reading::derived(
                format!("{base}.ecc"),
                serde_json::json!(ecc),
                None,
            ));
        } else {
            out.push(Reading::unavailable(
                format!("{base}.ecc"),
                None,
                "ECC is inferred from the data and total widths, and one is unknown",
            ));
        }
        push_spec_opt(
            out,
            format!("{base}.voltage"),
            dimm.voltage.map(|v| serde_json::json!(v)),
            Some(Unit::Volts),
            "SMBIOS reported no operating voltage",
        );
    }
}

fn resolve_cpu_cache(out: &mut Vec<Reading>) {
    const TOTALS: [&str; 4] = [
        "cpu.cache.l1d",
        "cpu.cache.l1i",
        "cpu.cache.l2",
        "cpu.cache.l3",
    ];

    let monitor = match crate::cpu_cache::CpuCacheMonitor::new() {
        Ok(m) => m,
        Err(e) => {
            for id in TOTALS {
                out.push(Reading::unavailable(
                    id,
                    Some(Unit::Bytes),
                    format!("cache topology unavailable: {e}"),
                ));
            }
            return;
        }
    };

    let topology = monitor.topology();
    // The platform sources state these in KiB. Converting here keeps every
    // capacity in the ontology in bytes, so an agent never has to ask which unit a
    // particular size field happens to use.
    let kib = |v: u64| (v > 0).then(|| serde_json::json!(v * 1024));
    for (id, value) in TOTALS.iter().zip([
        topology.total_l1d_kb,
        topology.total_l1i_kb,
        topology.total_l2_kb,
        topology.total_l3_kb,
    ]) {
        push_opt(
            out,
            *id,
            kib(value),
            Some(Unit::Bytes),
            "no aggregate was reported for this level; some platforms publish a \
             combined L1 rather than separate data and instruction figures, in \
             which case the per-instance entities below carry the sizes",
        );
    }

    for (i, cache) in monitor.caches().iter().enumerate() {
        let base = format!("cpu.cache.{i}");
        push_id(
            out,
            format!("{base}.level"),
            &format!("{:?}", cache.level).to_lowercase(),
        );
        push_opt(
            out,
            format!("{base}.size"),
            kib(cache.size_kb),
            Some(Unit::Bytes),
            "the platform reported no size for this cache",
        );
        push_opt(
            out,
            format!("{base}.line_size"),
            cache.line_size.map(|b| serde_json::json!(b)),
            Some(Unit::Bytes),
            "the platform reported no line size for this cache",
        );
        push_opt(
            out,
            format!("{base}.shared_cpus"),
            (!cache.shared_cpu_list.trim().is_empty())
                .then(|| serde_json::json!(cache.shared_cpu_list)),
            Some(Unit::Text),
            "the platform does not publish which processors share this cache",
        );
    }
}

fn resolve_gpu(out: &mut Vec<Reading>) {
    // All three failure paths report against `gpu.<none>`, the declared
    // diagnostic for a domain that enumerated nothing — not against `gpu.0.name`.
    //
    // Reporting them as an unavailable `gpu.0.name` asserted that adapter zero
    // exists and could not be read, when the truth is that no adapter is known
    // to exist at all. It also broke the schema's own promise: `gpu.{n}.name` is
    // declared non-nullable, meaning a GPU always has a name, so an unavailable
    // one is by definition a reader bug. On a headless CI runner that is exactly
    // what it was, and `non_nullable_entities_are_never_null` said so — on Linux,
    // where there is no GPU. Every machine that had one passed.
    let Ok(monitor) = crate::SiliconMonitor::new() else {
        out.push(Reading::unavailable(
            "gpu.<none>",
            None,
            "GPU enumeration failed",
        ));
        return;
    };
    let Ok(gpus) = monitor.snapshot_gpus() else {
        out.push(Reading::unavailable(
            "gpu.<none>",
            None,
            "GPU snapshot failed",
        ));
        return;
    };
    if gpus.is_empty() {
        // Absent hardware is a fact, not a failure — and not a zero-valued GPU.
        out.push(Reading::unavailable(
            "gpu.<none>",
            None,
            "no GPU detected on this machine",
        ));
        return;
    }

    for (i, gpu) in gpus.iter().enumerate() {
        let base = format!("gpu.{i}");
        push_text(out, format!("{base}.name"), &gpu.static_info.name);
        out.push(Reading::measured(
            format!("{base}.vendor"),
            serde_json::json!(format!("{:?}", gpu.static_info.vendor).to_lowercase()),
            Some(Unit::Identifier),
        ));

        let dynamic = &gpu.dynamic_info;
        // Not an Option: the collection layer already flattens an absent counter to
        // zero, so this cannot distinguish "idle" from "not reported" and must not
        // pretend to.
        out.push(Reading::measured(
            format!("{base}.utilization"),
            serde_json::json!(dynamic.utilization),
            Some(Unit::Percent),
        ));
        push_opt(
            out,
            format!("{base}.thermal.temperature"),
            dynamic.thermal.temperature.map(|t| serde_json::json!(t)),
            Some(Unit::Celsius),
            "no temperature sensor exposed for this adapter",
        );
        push_opt(
            out,
            format!("{base}.thermal.max_temperature"),
            dynamic
                .thermal
                .max_temperature
                .map(|t| serde_json::json!(t)),
            Some(Unit::Celsius),
            "vendor publishes no thermal limit for this adapter",
        );
        push_opt(
            out,
            format!("{base}.thermal.critical_temperature"),
            dynamic
                .thermal
                .critical_temperature
                .map(|t| serde_json::json!(t)),
            Some(Unit::Celsius),
            "vendor publishes no shutdown threshold for this adapter",
        );
        push_opt(
            out,
            format!("{base}.power.draw"),
            dynamic.power.draw.map(|p| serde_json::json!(p)),
            Some(Unit::Milliwatts),
            "driver reports no power telemetry",
        );
        push_opt(
            out,
            format!("{base}.power.limit"),
            dynamic.power.limit.map(|p| serde_json::json!(p)),
            Some(Unit::Milliwatts),
            "driver exposes no enforced power cap",
        );
        push_opt(
            out,
            format!("{base}.clocks.graphics"),
            dynamic.clocks.graphics.map(|c| serde_json::json!(c)),
            Some(Unit::Megahertz),
            "driver reports no graphics clock",
        );
        push_opt(
            out,
            format!("{base}.clocks.graphics.max"),
            dynamic.clocks.graphics_max.map(|c| serde_json::json!(c)),
            Some(Unit::Megahertz),
            "vendor publishes no graphics clock ceiling",
        );

        // Each figure stands on its own now. The old gate was `total > 0`,
        // which published *both* as unavailable whenever the total was missing
        // -- including the case where the used bytes were read perfectly well
        // and only the adapter's capacity was unknown, which is what the WMI
        // readers hit. It also had to infer absence from a zero, because the
        // zero was all the type could carry.
        let mem = &dynamic.memory;
        for (suffix, value) in [("total", mem.total), ("used", mem.used)] {
            match value {
                Some(bytes) => out.push(Reading::measured(
                    format!("{base}.memory.{suffix}"),
                    serde_json::json!(bytes),
                    Some(Unit::Bytes),
                )),
                None => out.push(Reading::unavailable(
                    format!("{base}.memory.{suffix}"),
                    Some(Unit::Bytes),
                    "the driver reported no figure for it. Unified-memory parts \
                     have no discrete video memory to report, and some adapters \
                     publish usage without publishing a capacity",
                )),
            }
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Make a device name safe to use as one segment of a dotted id.
///
/// Device names are chosen by vendors and drivers, not by simon: Windows hands back
/// "Bluetooth Network Connection", and VLAN interfaces carry dots. Both break the id
/// contract — a dot creates a spurious path segment, and whitespace makes the id
/// impossible to pass to `simon get` without quoting. Substitution is lossy but
/// total, which matters more here than being reversible: an id an agent cannot type
/// is not an id.
fn id_segment(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    // Collapse runs so "Bluetooth Network Connection" does not become a thicket of
    // underscores, and trim the edges.
    let mut out = String::with_capacity(cleaned.len());
    let mut last_underscore = false;
    for c in cleaned.chars() {
        if c == '_' {
            if !last_underscore {
                out.push(c);
            }
            last_underscore = true;
        } else {
            out.push(c);
            last_underscore = false;
        }
    }
    let trimmed = out.trim_matches('_').to_string();
    if trimmed.is_empty() {
        "unnamed".to_string()
    } else {
        trimmed
    }
}

/// Strings that name an absence rather than state a value.
///
/// DMI is the reason this exists: firmware tables routinely carry "n/a" or
/// "unknown" where a board vendor left a field unfilled, and passing those
/// through as `measured` hands an agent a reading that says nothing while
/// looking exactly like one that says something. `board.firmware.{n}.version`
/// did that on Linux CI runners, whose virtualised firmware fills two entries
/// with "n/a".
///
/// The list matches `unknown_is_never_dressed_as_a_measurement` in
/// `tests/ontology_conformance.rs`, which forbids these values in readings. It
/// is enforced here, at the one place every text reading passes through, rather
/// than in each reader — the conformance test has caught this class three times
/// now, always in a different reader.
const ABSENCE_WORDS: [&str; 5] = ["unknown", "unspecified", "undetermined", "n/a", "none"];

/// Public because the agent tool surface needs the *same* predicate, not a
/// second copy of the list. `ai_api::call_tool` applies it to every value it
/// returns; before that it had no guard at all, and `get_usb_devices` handed an
/// agent `"speed": "Unknown"` seventy times on one machine while the ontology
/// was rejecting that exact string.
pub fn names_an_absence(value: &str) -> bool {
    let v = value.trim().to_ascii_lowercase();
    ABSENCE_WORDS.contains(&v.as_str())
}

fn push_text(out: &mut Vec<Reading>, id: impl Into<String>, value: &str) {
    push_str_as(out, id, value, Unit::Text, Provenance::Measured);
}

/// `push_text` for a firmware-declared string. The absence handling is
/// identical; only the provenance differs, and it differs because a DIMM's
/// manufacturer was not measured off the module -- the SMBIOS table said so.
fn push_spec_text(out: &mut Vec<Reading>, id: impl Into<String>, value: &str) {
    push_str_as(out, id, value, Unit::Text, Provenance::Specification);
}

fn push_id(out: &mut Vec<Reading>, id: impl Into<String>, value: &str) {
    push_str_as(out, id, value, Unit::Identifier, Provenance::Measured);
}

/// `push_id` for a firmware-declared identifier.
fn push_spec_id(out: &mut Vec<Reading>, id: impl Into<String>, value: &str) {
    push_str_as(out, id, value, Unit::Identifier, Provenance::Specification);
}

/// The one place a string reading becomes a `Reading`.
///
/// Both guards below were added after the fact, each because a reader had
/// already tripped it. An empty string is an absence, and a string that spells
/// an absence -- an enum's `Unknown` variant lowercased, most often -- is also
/// an absence however confidently it arrives. Keeping this in one function is
/// why the second guard only had to be written once: the conformance test has
/// caught this class three times, in three different readers.
fn push_str_as(
    out: &mut Vec<Reading>,
    id: impl Into<String>,
    value: &str,
    unit: Unit,
    provenance: Provenance,
) {
    let id = id.into();
    if value.trim().is_empty() {
        out.push(Reading::unavailable(
            id,
            Some(unit),
            "reader returned an empty string",
        ));
        return;
    }
    if names_an_absence(value) {
        out.push(Reading::unavailable(
            id,
            Some(unit),
            format!(
                "reader returned {:?}, which names an absence rather than a value",
                value.trim()
            ),
        ));
        return;
    }
    let json = serde_json::json!(value);
    out.push(match provenance {
        Provenance::Specification => Reading::spec(id, json, Some(unit)),
        Provenance::Derived => Reading::derived(id, json, Some(unit)),
        _ => Reading::measured(id, json, Some(unit)),
    });
}

fn push_opt(
    out: &mut Vec<Reading>,
    id: impl Into<String>,
    value: Option<serde_json::Value>,
    unit: Option<Unit>,
    why_absent: &str,
) {
    match value {
        // A string that names an absence is an absence, however it arrived.
        // `push_text` and `push_id` were guarded first on the reasoning that
        // they were the only route a text reading could take; they were not.
        // `pci.*.link.speed` came through here as "unknown" with measured
        // provenance, on Linux, where PCIe link training is readable and some
        // devices report exactly that word.
        Some(v) if v.as_str().is_some_and(names_an_absence) => {
            out.push(Reading::unavailable(
                id,
                unit,
                format!(
                    "reader returned {}, which names an absence rather than a value",
                    v
                ),
            ));
        }
        Some(v) => out.push(Reading::measured(id, v, unit)),
        None => out.push(Reading::unavailable(id, unit, why_absent)),
    }
}

/// The platform CPU reader, keeping the error text.
///
/// `.ok()` threw it away, so every failure surfaced as the same sentence — "the
/// platform CPU reader returned an error" — with no way to tell a permission
/// problem from a parse failure from an unimplemented platform. There is a test
/// called `every_absence_carries_a_usable_reason`; a constant string satisfies
/// it and tells a reader nothing.
fn read_cpu_stats() -> Result<crate::core::cpu::CpuStats, String> {
    #[cfg(windows)]
    {
        crate::platform::windows::read_cpu_stats().map_err(|e| e.to_string())
    }
    #[cfg(target_os = "linux")]
    {
        crate::platform::linux::read_cpu_stats().map_err(|e| e.to_string())
    }
    #[cfg(target_os = "macos")]
    {
        crate::platform::macos::read_cpu_stats().map_err(|e| e.to_string())
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        Err("no CPU reader is implemented for this platform".to_string())
    }
}

/// The platform memory reader, keeping the error text. See [`read_cpu_stats`].
fn read_memory_stats() -> Result<crate::core::memory::MemoryStats, String> {
    #[cfg(windows)]
    {
        crate::platform::windows::read_memory_stats().map_err(|e| e.to_string())
    }
    #[cfg(target_os = "linux")]
    {
        crate::platform::linux::read_memory_stats().map_err(|e| e.to_string())
    }
    #[cfg(target_os = "macos")]
    {
        crate::platform::macos::read_memory_stats().map_err(|e| e.to_string())
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        Err("no memory reader is implemented for this platform".to_string())
    }
}

fn detect_board() -> Option<crate::core::platform_info::BoardInfo> {
    #[cfg(windows)]
    {
        crate::platform::windows::detect_platform().ok()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Check a snapshot against the ontology's declared ranges.
///
/// Returns one message per reading that is physically impossible. An empty result
/// is the assertion this repository's plausibility tier makes, available to any
/// consumer rather than only to the test suite.
pub fn validate(readings: &[Reading]) -> Vec<String> {
    let ontology = Ontology::build();
    let mut problems = Vec::new();
    for r in readings {
        let Some(value) = r.value.as_ref().and_then(|v| v.as_f64()) else {
            continue;
        };
        if let Some(entity) = lookup_template(&ontology, &r.id) {
            if let Some(problem) = entity.validate_range(value) {
                problems.push(problem);
            }
        }
    }
    problems
}

/// Find the entity for a concrete id, matching `gpu.0.name` against `gpu.{n}.name`.
fn lookup_template<'a>(ontology: &'a Ontology, concrete: &str) -> Option<&'a Entity> {
    ontology.template_for(concrete)
}

/// Number of processes reported. Enumerating every process would make a snapshot
/// dominated by transient noise, so the list is capped — and the cap is announced as
/// a reading rather than applied silently, because a truncated list that looks
/// complete is its own kind of wrong answer.
const PROCESS_LIMIT: usize = 10;

fn resolve_disk(out: &mut Vec<Reading>) {
    let disks = match crate::disk::enumerate_disks() {
        Ok(d) => d,
        Err(e) => {
            out.push(Reading::unavailable(
                "disk.<none>",
                None,
                format!("disk enumeration failed: {e}"),
            ));
            return;
        }
    };
    if disks.is_empty() {
        out.push(Reading::unavailable(
            "disk.<none>",
            None,
            "no block devices enumerated on this machine",
        ));
        return;
    }

    for (i, disk) in disks.iter().enumerate() {
        let base = format!("disk.{i}");
        match disk.info() {
            Ok(info) => {
                push_text(out, format!("{base}.model"), &info.model);
                if info.capacity > 0 {
                    out.push(Reading::measured(
                        format!("{base}.capacity"),
                        serde_json::json!(info.capacity),
                        Some(Unit::Bytes),
                    ));
                } else {
                    out.push(Reading::unavailable(
                        format!("{base}.capacity"),
                        Some(Unit::Bytes),
                        "device reported zero capacity",
                    ));
                }
            }
            Err(e) => {
                for suffix in ["model", "capacity"] {
                    out.push(Reading::unavailable(
                        format!("{base}.{suffix}"),
                        None,
                        format!("device info read failed: {e}"),
                    ));
                }
            }
        }

        // Throughput is documented as calculated from recent samples. One sample is
        // not two, so a single-shot query legitimately has none.
        match disk.io_stats() {
            Ok(io) => {
                push_opt(
                    out,
                    format!("{base}.read_rate"),
                    io.read_throughput.map(|v| serde_json::json!(v)),
                    Some(Unit::BytesPerSecond),
                    "a throughput needs two samples; this query took one",
                );
                push_opt(
                    out,
                    format!("{base}.write_rate"),
                    io.write_throughput.map(|v| serde_json::json!(v)),
                    Some(Unit::BytesPerSecond),
                    "a throughput needs two samples; this query took one",
                );
            }
            Err(e) => {
                for suffix in ["read_rate", "write_rate"] {
                    out.push(Reading::unavailable(
                        format!("{base}.{suffix}"),
                        Some(Unit::BytesPerSecond),
                        format!("I/O statistics read failed: {e}"),
                    ));
                }
            }
        }

        resolve_disk_health(out, &base, disk.as_ref());
    }
}

/// Health, SMART and NVMe entities for one drive.
///
/// Split out because the three readers fail independently and each has its own
/// reason to have nothing to say — a SATA drive has no NVMe log page, an NVMe
/// drive has no sector reallocation concept, and a USB bridge may tunnel neither.
/// An agent needs those told apart, so each absence carries the reason rather than
/// the row being omitted.
fn resolve_disk_health(out: &mut Vec<Reading>, base: &str, disk: &dyn crate::disk::DiskDevice) {
    // Spelled out rather than derived from `{:?}`, which lowercases `NvmeSsd` to
    // `nvmessd` — a token no reader would guess and no id elsewhere resembles.
    use crate::disk::DiskType;
    let kind = match disk.disk_type() {
        DiskType::NvmeSsd => Some("nvme_ssd"),
        DiskType::SataSsd => Some("sata_ssd"),
        DiskType::SataHdd => Some("sata_hdd"),
        DiskType::Scsi => Some("scsi"),
        DiskType::Usb => Some("usb"),
        DiskType::Virtual => Some("virtual"),
        DiskType::Unknown => None,
    };
    push_opt(
        out,
        format!("{base}.kind"),
        kind.map(|k| serde_json::json!(k)),
        Some(Unit::Text),
        "the platform did not classify this device's transport or medium",
    );

    match disk.info() {
        Ok(info) => push_opt(
            out,
            format!("{base}.serial"),
            info.serial.as_ref().map(|s| serde_json::json!(s)),
            Some(Unit::Text),
            "the platform did not disclose a serial number for this device",
        ),
        Err(e) => out.push(Reading::unavailable(
            format!("{base}.serial"),
            Some(Unit::Text),
            format!("device info read failed: {e}"),
        )),
    }

    // `Unknown` is the absence of a verdict, not a verdict of "unknown". Passing it
    // through as a measured string would let an agent record a health check it
    // never actually got — the same shape as reporting an access error as 0 °C.
    use crate::disk::DiskHealth;
    match disk.health() {
        Ok(DiskHealth::Unknown) => out.push(Reading::unavailable(
            format!("{base}.health"),
            Some(Unit::Text),
            "no counter this device exposes yielded a health verdict",
        )),
        Ok(h) => push_text(
            out,
            format!("{base}.health"),
            &format!("{h:?}").to_lowercase(),
        ),
        Err(e) => out.push(Reading::unavailable(
            format!("{base}.health"),
            Some(Unit::Text),
            format!("health read failed: {e}"),
        )),
    }

    const SMART_FIELDS: [&str; 7] = [
        "temperature",
        "smart.passed",
        "smart.power_on_hours",
        "smart.power_cycles",
        "smart.reallocated_sectors",
        "smart.pending_sectors",
        "smart.uncorrectable_sectors",
    ];

    match disk.smart_info() {
        Ok(s) => {
            push_opt(
                out,
                format!("{base}.temperature"),
                s.temperature.map(|t| serde_json::json!(t)),
                Some(Unit::Celsius),
                "this device exposes no thermal sensor",
            );
            push_opt(
                out,
                format!("{base}.smart.passed"),
                s.passed.map(|v| serde_json::json!(v)),
                None,
                concat!(
                    "the drive's own verdict was not obtained: neither the NVMe ",
                    "critical warning bits nor the ATA failure prediction was ",
                    "readable, and this field is not a judgement computed from ",
                    "the counters"
                ),
            );
            push_opt(
                out,
                format!("{base}.smart.power_on_hours"),
                s.power_on_hours.map(|v| serde_json::json!(v)),
                Some(Unit::Hours),
                "the drive did not report a power-on hour count",
            );
            push_opt(
                out,
                format!("{base}.smart.power_cycles"),
                s.power_cycle_count.map(|v| serde_json::json!(v)),
                Some(Unit::Count),
                "the drive did not report a power cycle count",
            );
            // NVMe has no sector reallocation concept. Reporting 0 here would
            // assert a clean count that was never measured, which is exactly the
            // substitution this module exists to refuse.
            push_opt(
                out,
                format!("{base}.smart.reallocated_sectors"),
                s.reallocated_sectors.map(|v| serde_json::json!(v)),
                Some(Unit::Count),
                "not an NVMe concept; on ATA, the drive did not report attribute 5",
            );
            push_opt(
                out,
                format!("{base}.smart.pending_sectors"),
                s.pending_sectors.map(|v| serde_json::json!(v)),
                Some(Unit::Count),
                "not an NVMe concept; on ATA, the drive did not report attribute 197",
            );
            push_opt(
                out,
                format!("{base}.smart.uncorrectable_sectors"),
                s.uncorrectable_sectors.map(|v| serde_json::json!(v)),
                Some(Unit::Count),
                "the drive did not report an uncorrectable count",
            );
        }
        Err(e) => {
            // On Windows this is the unelevated case for devices that reach the
            // WMI fallback, and the error says so. Naming the reason is the whole
            // point: a caller can tell "needs elevation" from "no such sensor".
            for suffix in SMART_FIELDS {
                out.push(Reading::unavailable(
                    format!("{base}.{suffix}"),
                    None,
                    format!("SMART read failed: {e}"),
                ));
            }
        }
    }

    const NVME_FIELDS: [&str; 6] = [
        "nvme.version",
        "nvme.percentage_used",
        "nvme.data_units_written",
        "nvme.data_units_read",
        "nvme.power_state",
        "nvme.critical_warnings",
    ];

    match disk.nvme_info() {
        Ok(n) => {
            push_opt(
                out,
                format!("{base}.nvme.version"),
                n.nvme_version.as_ref().map(|v| serde_json::json!(v)),
                Some(Unit::Text),
                "the controller did not report a specification version",
            );
            push_opt(
                out,
                format!("{base}.nvme.percentage_used"),
                n.percentage_used.map(|v| serde_json::json!(v)),
                Some(Unit::Percent),
                "the controller did not report a wear figure",
            );
            push_opt(
                out,
                format!("{base}.nvme.data_units_written"),
                n.data_units_written.map(|v| serde_json::json!(v)),
                Some(Unit::Count),
                "the controller did not report a written data unit count",
            );
            push_opt(
                out,
                format!("{base}.nvme.data_units_read"),
                n.data_units_read.map(|v| serde_json::json!(v)),
                Some(Unit::Count),
                "the controller did not report a read data unit count",
            );
            push_opt(
                out,
                format!("{base}.nvme.power_state"),
                n.power_state.map(|v| serde_json::json!(v)),
                Some(Unit::Count),
                "the controller does not implement Get Features for power management",
            );
            push_opt(
                out,
                format!("{base}.nvme.critical_warnings"),
                n.critical_warnings.map(|v| serde_json::json!(v)),
                Some(Unit::Count),
                "the controller did not serve the SMART/Health log page",
            );
        }
        Err(e) => {
            // `NotSupported` here is the device declining the NVMe protocol, which
            // is how a SATA or USB device answers. That is a fact about the device,
            // not a failure of the query, and the note says which.
            for suffix in NVME_FIELDS {
                out.push(Reading::unavailable(
                    format!("{base}.{suffix}"),
                    None,
                    format!("no NVMe controller data: {e}"),
                ));
            }
        }
    }
}

fn resolve_network(out: &mut Vec<Reading>) {
    let mut monitor = match crate::network_monitor::NetworkMonitor::new() {
        Ok(m) => m,
        Err(e) => {
            out.push(Reading::unavailable(
                "network.<none>",
                None,
                format!("network monitor unavailable: {e}"),
            ));
            return;
        }
    };
    let interfaces = match monitor.interfaces() {
        Ok(i) => i,
        Err(e) => {
            out.push(Reading::unavailable(
                "network.<none>",
                None,
                format!("interface enumeration failed: {e}"),
            ));
            return;
        }
    };
    if interfaces.is_empty() {
        out.push(Reading::unavailable(
            "network.<none>",
            None,
            "no network interfaces enumerated",
        ));
        return;
    }

    for iface in &interfaces {
        let base = format!("network.{}", id_segment(&iface.name));

        out.push(Reading::measured(
            format!("{base}.rx_bytes"),
            serde_json::json!(iface.rx_bytes),
            Some(Unit::Bytes),
        ));
        out.push(Reading::measured(
            format!("{base}.tx_bytes"),
            serde_json::json!(iface.tx_bytes),
            Some(Unit::Bytes),
        ));

        // The counters above are cumulative. Reporting them as a rate would be the
        // same category of error as calling a spec constant a measurement.
        for dir in ["rx", "tx"] {
            out.push(Reading::unavailable(
                format!("{base}.{dir}_rate"),
                Some(Unit::BytesPerSecond),
                "a rate needs two samples; this query took one — differentiate \
                 rx_bytes/tx_bytes across two snapshots",
            ));
        }

        push_opt(
            out,
            format!("{base}.link_speed"),
            iface.speed_mbps.map(|s| serde_json::json!(s)),
            Some(Unit::Count),
            "driver reports no negotiated link rate for this interface",
        );
    }
}

fn resolve_process(out: &mut Vec<Reading>) {
    let mut monitor = match crate::process_monitor::ProcessMonitor::new() {
        Ok(m) => m,
        Err(e) => {
            out.push(Reading::unavailable(
                "process.<none>",
                None,
                format!("process monitor unavailable: {e}"),
            ));
            return;
        }
    };
    let procs = match monitor.processes_by_memory() {
        Ok(p) => p,
        Err(e) => {
            out.push(Reading::unavailable(
                "process.<none>",
                None,
                format!("process enumeration failed: {e}"),
            ));
            return;
        }
    };
    if procs.is_empty() {
        out.push(Reading::unavailable(
            "process.<none>",
            None,
            "no processes enumerated",
        ));
        return;
    }

    let total = procs.len();
    for p in procs.iter().take(PROCESS_LIMIT) {
        let base = format!("process.{}", p.pid);
        push_text(out, format!("{base}.name"), &p.name);
        out.push(Reading::measured(
            format!("{base}.cpu"),
            serde_json::json!(p.cpu_percent),
            Some(Unit::Percent),
        ));
        out.push(Reading::measured(
            format!("{base}.memory"),
            serde_json::json!(p.memory_bytes),
            Some(Unit::Bytes),
        ));
    }

    // Announce the truncation. A capped list presented as complete would let an agent
    // conclude a process is absent when it was merely ranked eleventh.
    if total > PROCESS_LIMIT {
        out.push(Reading::unavailable(
            "process.<truncated>",
            Some(Unit::Count),
            format!(
                "{total} processes exist; this snapshot reports the {PROCESS_LIMIT} \
                 largest by memory. Absence from this list is not absence from the \
                 machine — use `simon cli processes` for the full table"
            ),
        ));
    }
}

fn resolve_thermal(out: &mut Vec<Reading>) {
    let temps = match crate::motherboard::get_system_temperatures() {
        Ok(t) => t,
        Err(e) => {
            out.push(Reading::unavailable(
                "thermal.<none>",
                Some(Unit::Celsius),
                format!("temperature read failed: {e}"),
            ));
            return;
        }
    };

    for (name, value) in [
        ("cpu", temps.cpu),
        ("gpu", temps.gpu),
        ("motherboard", temps.motherboard),
    ] {
        match value {
            Some(v) => out.push(Reading::measured(
                format!("thermal.{name}.temperature"),
                serde_json::json!(v),
                Some(Unit::Celsius),
            )),
            None => out.push(Reading::unavailable(
                format!("thermal.{name}.temperature"),
                Some(Unit::Celsius),
                "no sensor exposed for this component — on Windows most board \
                 sensors require a signed kernel driver, and a virtual machine \
                 usually exposes none at all",
            )),
        }
    }

    for (device, value) in temps.storage.iter().chain(temps.network.iter()) {
        out.push(Reading::measured(
            format!("thermal.{}.temperature", id_segment(device)),
            serde_json::json!(value),
            Some(Unit::Celsius),
        ));
    }

    // There is deliberately no `thermal.<none>` row when every sensor comes back
    // empty. `<none>` means the domain enumerated nothing, and this resolver always
    // enumerates cpu, gpu and motherboard — it emits an `unavailable` row for each,
    // which states the same fact per component and more precisely. Emitting both
    // produced a snapshot that said "the domain enumerated nothing" beside three
    // thermal rows. It only showed up on a machine where no sensor reads at all,
    // which is why the first macOS CI run found it and no Windows run ever did.
    //
    // The read-failure path above still emits `<none>`, and correctly: it returns
    // before pushing anything, so nothing contradicts it.
}

/// Attached displays.
fn resolve_displays(out: &mut Vec<Reading>) {
    let monitor = match crate::display::DisplayMonitor::new() {
        Ok(m) => m,
        Err(e) => {
            out.push(Reading::unavailable(
                "board.display.<none>",
                None,
                format!("display enumeration failed: {e}"),
            ));
            return;
        }
    };

    let displays = monitor.displays();
    if displays.is_empty() {
        out.push(Reading::unavailable(
            "board.display.<none>",
            None,
            concat!(
                "no display is attached, or none is visible to this session — a ",
                "headless server and a locked-down service account both look like this",
            ),
        ));
        return;
    }

    for (i, d) in displays.iter().enumerate() {
        let base = format!("board.display.{i}");
        push_opt(
            out,
            format!("{base}.name"),
            d.name.as_ref().map(|n| serde_json::json!(n)),
            Some(Unit::Text),
            "this display publishes no name",
        );
        push_id(
            out,
            format!("{base}.connection"),
            &format!("{:?}", d.connection).to_lowercase(),
        );
        // A display is not zero pixels wide. This used to filter `> 0` because
        // the reader wrote zeros for a monitor whose mode it could not read;
        // the fields are `Option` now, so the absence arrives here intact
        // instead of being reconstructed from a sentinel. The zero guard stays
        // because a reader could still produce one, and it would mean the same
        // thing.
        const NOT_A_MODE: &str = "the display is attached and its current mode was not readable; zero is not a resolution";
        push_opt(
            out,
            format!("{base}.width"),
            d.width.filter(|w| *w > 0).map(|w| serde_json::json!(w)),
            Some(Unit::Count),
            NOT_A_MODE,
        );
        push_opt(
            out,
            format!("{base}.height"),
            d.height.filter(|h| *h > 0).map(|h| serde_json::json!(h)),
            Some(Unit::Count),
            NOT_A_MODE,
        );
        push_opt(
            out,
            format!("{base}.refresh_rate"),
            d.refresh_rate
                .filter(|r| *r > 0.0)
                .map(|r| serde_json::json!(r)),
            Some(Unit::Hertz),
            NOT_A_MODE,
        );
        out.push(Reading::measured(
            format!("{base}.primary"),
            serde_json::json!(d.is_primary),
            None,
        ));
    }
}

/// Platform sensor devices — ambient light, accelerometer, orientation.
///
/// Not the board temperature sensors, which resolve under `thermal`. A desktop
/// reporting none is the common case and a true reading, so the diagnostic
/// distinguishes that from a query that could not be made at all.
fn resolve_sensors(out: &mut Vec<Reading>) {
    let monitor = match crate::sensors::SensorMonitor::new() {
        Ok(m) => m,
        Err(e) => {
            out.push(Reading::unavailable(
                "board.sensor.<none>",
                None,
                format!("sensor enumeration failed: {e}"),
            ));
            return;
        }
    };

    let items = monitor.sensors();
    if items.is_empty() {
        out.push(Reading::unavailable(
            "board.sensor.<none>",
            None,
            match monitor.note() {
                Some(why) => why.to_string(),
                // No note and no items: the enumeration ran and this machine has
                // none, which is the ordinary case on a desktop.
                None => "the platform enumerated no sensors on this machine".to_string(),
            },
        ));
        return;
    }

    for (i, sensor) in items.iter().enumerate() {
        let base = format!("board.sensor.{i}");
        push_text(out, format!("{base}.name"), &sensor.name);
        push_id(
            out,
            format!("{base}.type"),
            &format!("{:?}", sensor.sensor_type).to_lowercase(),
        );
        out.push(Reading::measured(
            format!("{base}.active"),
            serde_json::json!(sensor.active),
            None,
        ));
    }
}

/// Whether a string is a PCI address in either of the two forms that appear.
///
/// Domain-qualified `0000:03:00.0` and the bare `03:00.0` are both accepted;
/// anything else -- a Windows device instance path, a SCSI target triple, an
/// empty string -- is not. The check is deliberately shallow: it exists to stop
/// a value being published under a name that promises a PCI bus when it names
/// something else, not to validate that the address points at a real device.
fn looks_like_pci_address(value: &str) -> bool {
    let value = value.trim();
    let (bus_dev, function) = match value.rsplit_once('.') {
        Some(parts) => parts,
        None => return false,
    };
    if function.is_empty() || !function.chars().all(|c| c.is_ascii_hexdigit()) {
        return false;
    }
    let fields: Vec<&str> = bus_dev.split(':').collect();
    if !matches!(fields.len(), 2 | 3) {
        return false;
    }
    fields
        .iter()
        .all(|f| !f.is_empty() && f.chars().all(|c| c.is_ascii_hexdigit()))
}

/// The controllers the disks are attached to.
fn resolve_storage_controllers(out: &mut Vec<Reading>) {
    let monitor = match crate::storage_controller::StorageControllerMonitor::new() {
        Ok(m) => m,
        Err(e) => {
            out.push(Reading::unavailable(
                "disk.controller.<none>",
                None,
                format!("storage controllers could not be enumerated here: {e}"),
            ));
            return;
        }
    };

    let controllers = monitor.controllers();
    if controllers.is_empty() {
        out.push(Reading::unavailable(
            "disk.controller.<none>",
            None,
            "controller enumeration succeeded and found none, which on a machine with \
             disks means the enumeration is incomplete rather than that the disks are \
             attached to nothing",
        ));
        return;
    }

    for (i, c) in controllers.iter().enumerate() {
        let base = format!("disk.controller.{i}");
        push_text(out, format!("{base}.name"), &c.name);
        push_text(out, format!("{base}.vendor"), &c.vendor);
        push_text(out, format!("{base}.model"), &c.model);
        push_id(out, format!("{base}.driver"), &c.driver);
        push_id(
            out,
            format!("{base}.interface"),
            &format!("{:?}", c.interface),
        );
        // Windows fills this field with a device instance path --
        // `ROOT\\SPACEPORT\\0000` for a Storage Spaces controller -- which is
        // not a PCI address and cannot be joined against `pci.*`. Publishing it
        // under a name that says PCI would send a consumer looking for a link
        // width to a bus the device is not on. Only a value that looks like one
        // is published as one, and the rest arrive as an absence naming what
        // was actually found.
        push_opt(
            out,
            format!("{base}.pci_address"),
            looks_like_pci_address(&c.pci_address).then(|| serde_json::json!(c.pci_address)),
            Some(Unit::Identifier),
            if c.pci_address.trim().is_empty() {
                "this controller reports no bus address".to_string()
            } else {
                format!(
                    "the platform reports `{}` for this controller, which is not a PCI \
                     address and cannot be joined against `pci.*`",
                    c.pci_address.trim()
                )
            }
            .as_str(),
        );
        push_opt(
            out,
            format!("{base}.ports"),
            (c.ports > 0).then(|| serde_json::json!(c.ports)),
            Some(Unit::Count),
            "this controller reports no port count",
        );
    }
}

/// Operating system power plans, and which one is in force.
fn resolve_power_profiles(out: &mut Vec<Reading>) {
    let monitor = match crate::power_profile::PowerProfileMonitor::new() {
        Ok(m) => m,
        Err(e) => {
            out.push(Reading::unavailable(
                "power.profile.<none>",
                None,
                format!("power plans could not be enumerated here: {e}"),
            ));
            return;
        }
    };

    let plans = monitor.power_plans();
    if plans.is_empty() {
        out.push(Reading::unavailable(
            "power.profile.<none>",
            None,
            "no power plan was enumerated; this platform may manage power without \
             presenting named plans",
        ));
        return;
    }

    for (i, p) in plans.iter().enumerate() {
        let base = format!("power.profile.{i}");
        push_text(out, format!("{base}.name"), &p.name);
        out.push(Reading::measured(
            format!("{base}.active"),
            serde_json::json!(p.active),
            None,
        ));
    }
}

/// Hardware video encode and decode, each row carrying how it was learned.
///
/// The reader distinguishes a capability it asked the driver about from one it
/// concluded from the GPU model, and that distinction survives into the
/// provenance of every reading below. It is the one place in this module where
/// the provenance is chosen per row rather than per entity, and it is the
/// clearest illustration of why the field exists: an inferred AV1 encode
/// capability and a queried one look identical until you ask where each came
/// from, and only one of them will still be true after a driver update.
fn resolve_codecs(out: &mut Vec<Reading>) {
    use crate::codec::{BitDepth, CapabilitySource, MaxResolution};

    let monitor = match crate::codec::CodecMonitor::new() {
        Ok(m) => m,
        Err(e) => {
            out.push(Reading::unavailable(
                "gpu.codec.<none>",
                None,
                format!("hardware codec support could not be determined here: {e}"),
            ));
            return;
        }
    };

    let caps = monitor.capabilities();
    if caps.is_empty() {
        out.push(Reading::unavailable(
            "gpu.codec.<none>",
            None,
            "no hardware codec capability was found; the GPU may have no media engine",
        ));
        return;
    }

    for (i, c) in caps.iter().enumerate() {
        let base = format!("gpu.codec.{i}");
        let queried = c.source == CapabilitySource::DirectQuery;

        // Every identity field on this row is exactly as good as the row's
        // source. Pushing them all as measured would have hidden that behind a
        // `confidence` number a consumer has to remember to read.
        let push_str = |out: &mut Vec<Reading>, id: String, v: &str| {
            if queried {
                push_id(out, id, v);
            } else {
                push_spec_id(out, id, v);
            }
        };

        if queried {
            push_text(out, format!("{base}.device"), &c.device);
        } else {
            push_spec_text(out, format!("{base}.device"), &c.device);
        }
        push_str(out, format!("{base}.codec"), &format!("{:?}", c.codec));
        push_str(
            out,
            format!("{base}.direction"),
            &format!("{:?}", c.direction),
        );
        push_str(out, format!("{base}.engine"), &c.engine);
        push_str(
            out,
            format!("{base}.max_resolution"),
            &format!("{:?}", c.max_resolution),
        );

        // Pixel dimensions for the frame class. A table, and declared as one:
        // the class is what the reader knows, and these two are what a consumer
        // sizing a buffer actually needs.
        let pixels = match c.max_resolution {
            MaxResolution::SD => Some((720, 480)),
            MaxResolution::HD => Some((1280, 720)),
            MaxResolution::FullHD => Some((1920, 1080)),
            MaxResolution::QHD => Some((2560, 1440)),
            MaxResolution::UHD4K => Some((3840, 2160)),
            MaxResolution::UHD8K => Some((7680, 4320)),
            MaxResolution::Unknown => None,
        };
        match pixels {
            Some((w, h)) => {
                out.push(Reading::derived(
                    format!("{base}.max_width"),
                    serde_json::json!(w),
                    Some(Unit::Count),
                ));
                out.push(Reading::derived(
                    format!("{base}.max_height"),
                    serde_json::json!(h),
                    Some(Unit::Count),
                ));
            }
            None => {
                for suffix in ["max_width", "max_height"] {
                    out.push(Reading::unavailable(
                        format!("{base}.{suffix}"),
                        Some(Unit::Count),
                        "the engine reports no frame class, so no dimensions follow from it",
                    ));
                }
            }
        }

        let depth = match c.max_bit_depth {
            BitDepth::Bit8 => Some(8),
            BitDepth::Bit10 => Some(10),
            BitDepth::Bit12 => Some(12),
            BitDepth::Unknown => None,
        };
        match depth {
            Some(d) if queried => out.push(Reading::measured(
                format!("{base}.max_bit_depth"),
                serde_json::json!(d),
                Some(Unit::Count),
            )),
            Some(d) => out.push(Reading::spec(
                format!("{base}.max_bit_depth"),
                serde_json::json!(d),
                Some(Unit::Count),
            )),
            None => out.push(Reading::unavailable(
                format!("{base}.max_bit_depth"),
                Some(Unit::Count),
                "the engine reports no bit depth",
            )),
        }

        // The comment that used to sit here said this figure was "arithmetic
        // over the engine generation". There was no arithmetic: `max_fps` was
        // the literal 60 at every construction site, so the `0 =>` arm that
        // guarded it was unreachable and the entity's declared derivation from
        // `codec` and `max_resolution` described a computation that did not
        // exist. Nothing fills the field now, and the entity is declared
        // `Unavailable` to match; a reader that learns a real frame rate has to
        // re-declare it, with the inputs it actually consumes.
        push_opt(
            out,
            format!("{base}.max_fps"),
            c.max_fps.map(|fps| serde_json::json!(fps)),
            Some(Unit::Count),
            "no driver reports a frame rate and this crate does not estimate one",
        );
        // The reader's confidence is a 0.0-1.0 fraction and the entity declares
        // a percentage, so it is scaled here rather than at the consumer, where
        // a 1.0 would read as one percent.
        out.push(Reading::derived(
            format!("{base}.confidence"),
            serde_json::json!(c.confidence * 100.0),
            Some(Unit::Percent),
        ));
    }
}

/// Keyboards, pointing devices and controllers, as attached right now.
fn resolve_input(out: &mut Vec<Reading>) {
    let monitor = match crate::input::InputMonitor::new() {
        Ok(m) => m,
        Err(e) => {
            out.push(Reading::unavailable(
                "board.input.<none>",
                None,
                format!("input devices could not be enumerated here: {e}"),
            ));
            return;
        }
    };

    let devices = monitor.devices();
    if devices.is_empty() {
        out.push(Reading::unavailable(
            "board.input.<none>",
            None,
            "input enumeration succeeded and found no device, which is ordinary on a \
             headless machine",
        ));
        return;
    }

    for (i, d) in devices.iter().enumerate() {
        let base = format!("board.input.{i}");
        push_text(out, format!("{base}.name"), &d.name);
        push_id(out, format!("{base}.type"), &format!("{:?}", d.device_type));
        push_id(
            out,
            format!("{base}.interface"),
            &format!("{:?}", d.interface),
        );
        push_text(out, format!("{base}.vendor"), &d.vendor);
        push_text(out, format!("{base}.product"), &d.product);
        push_opt(
            out,
            format!("{base}.active"),
            d.is_active.map(|a| serde_json::json!(a)),
            None,
            "the platform did not report whether this device is connected and usable",
        );
    }
}

/// Audio endpoints, in whichever direction they carry sound.
fn resolve_audio(out: &mut Vec<Reading>) {
    let monitor = match crate::audio::AudioMonitor::new() {
        Ok(m) => m,
        Err(e) => {
            out.push(Reading::unavailable(
                "board.audio.<none>",
                None,
                format!("audio endpoints could not be enumerated here: {e}"),
            ));
            return;
        }
    };

    let devices = monitor.devices();
    if devices.is_empty() {
        out.push(Reading::unavailable(
            "board.audio.<none>",
            None,
            "audio enumeration succeeded and found no endpoint",
        ));
        return;
    }

    for (i, d) in devices.iter().enumerate() {
        let base = format!("board.audio.{i}");
        push_text(out, format!("{base}.name"), &d.name);
        push_id(
            out,
            format!("{base}.direction"),
            &format!("{:?}", d.device_type),
        );
        push_opt(
            out,
            format!("{base}.state"),
            d.state
                .map(|s| serde_json::json!(format!("{s:?}").to_lowercase())),
            Some(Unit::Identifier),
            "the platform did not report an endpoint state for this device",
        );
        push_opt(
            out,
            format!("{base}.default"),
            d.is_default.map(|v| serde_json::json!(v)),
            None,
            concat!(
                "which endpoint the system routes to by default is a COM call ",
                "this crate does not make; it was previously whichever row the ",
                "enumeration returned first"
            ),
        );
        // Neither of these is read on any platform yet. The volume row already
        // said so; the mute row published a constant `false` as `measured`,
        // and the conformance suite caught it the moment the field could
        // express an absence.
        const NO_MIXER: &str =
            "simon has no mixer binding on this platform, so no level is read from this endpoint";
        push_opt(
            out,
            format!("{base}.volume"),
            d.volume.map(|v| serde_json::json!(v)),
            Some(Unit::Percent),
            NO_MIXER,
        );
        push_opt(
            out,
            format!("{base}.muted"),
            d.muted.map(|m| serde_json::json!(m)),
            None,
            NO_MIXER,
        );
    }
}

/// Cameras, and whether any of them is streaming.
fn resolve_cameras(out: &mut Vec<Reading>) {
    let monitor = match crate::camera::CameraMonitor::new() {
        Ok(m) => m,
        Err(e) => {
            out.push(Reading::unavailable(
                "board.camera.<none>",
                None,
                format!("cameras could not be enumerated here: {e}"),
            ));
            return;
        }
    };

    let cameras = monitor.cameras();
    if cameras.is_empty() {
        out.push(Reading::unavailable(
            "board.camera.<none>",
            None,
            "camera enumeration succeeded and found no device",
        ));
        return;
    }

    for (i, c) in cameras.iter().enumerate() {
        let base = format!("board.camera.{i}");
        push_text(out, format!("{base}.name"), &c.name);
        push_id(
            out,
            format!("{base}.connection"),
            &format!("{:?}", c.connection),
        );
        push_id(out, format!("{base}.driver"), &c.driver);
        // Zero pixels is not a frame size, so it is reported absent -- but the
        // reason used to say "this camera reports no supported mode list",
        // which is a claim about the device, and no reader on any platform has
        // ever asked it. The Linux path carries the comment "would need v4l2
        // ioctl", Windows and macOS fill zero without comment, and the
        // ontology attributed all three silences to the camera.
        const NO_MODE_LIST: &str = concat!(
            "no reader asks this camera for its mode list on any platform: the ",
            "Linux path would need the VIDIOC_ENUM_FRAMESIZES ioctl and the ",
            "Windows path the Media Foundation frame-size attribute, and ",
            "neither is implemented. This is simon not looking, not a camera ",
            "that answered with nothing"
        );
        push_opt(
            out,
            format!("{base}.max_width"),
            (c.max_width > 0).then(|| serde_json::json!(c.max_width)),
            Some(Unit::Count),
            NO_MODE_LIST,
        );
        push_opt(
            out,
            format!("{base}.max_height"),
            (c.max_height > 0).then(|| serde_json::json!(c.max_height)),
            Some(Unit::Count),
            NO_MODE_LIST,
        );
        push_opt(
            out,
            format!("{base}.active"),
            c.is_active.map(|a| serde_json::json!(a)),
            None,
            concat!(
                "whether this camera is streaming was not established: only ",
                "Linux probes the device node, and the Windows consent store ",
                "attributes use to an application rather than to a device"
            ),
        );
    }
}

/// How long the machine took to boot, when the platform measured it.
///
/// Bound late: the entity was declared and nothing resolved it, so it read "no
/// resolver bound on this build" — which was the wrong absence. The entity is
/// supported on Windows; what stops it here is privilege, and saying so is a
/// different fact than saying simon has not implemented it.
///
/// `BootMonitor` leaves `total` at `Duration::ZERO` when nothing was measured,
/// so zero is the sentinel and must not be published as a boot that took no
/// time. The reason given for the absence is platform-specific because the
/// reasons genuinely differ: Windows keeps the only real measurement in a log
/// that needs Administrator, while elsewhere it comes from `systemd-analyze`,
/// which is simply absent on a machine without systemd.
fn resolve_boot_duration(out: &mut Vec<Reading>) {
    let monitor = match crate::boot_config::BootMonitor::new() {
        Ok(m) => m,
        Err(e) => {
            out.push(Reading::unavailable(
                "system.boot.duration",
                Some(Unit::Seconds),
                format!("the boot configuration could not be read: {e}"),
            ));
            return;
        }
    };

    let secs = monitor.boot_time.total.as_secs_f64();
    if secs > 0.0 {
        out.push(Reading::measured(
            "system.boot.duration",
            serde_json::json!(secs),
            Some(Unit::Seconds),
        ));
        return;
    }

    #[cfg(target_os = "windows")]
    let reason = concat!(
        "Windows records a boot duration only in ",
        "Microsoft-Windows-Diagnostics-Performance/Operational event 100, which ",
        "is not readable without Administrator — this is a privilege ",
        "boundary, not a missing reader",
    );
    #[cfg(target_os = "linux")]
    let reason = concat!(
        "`systemd-analyze` reported no total, which is what a machine without ",
        "systemd or with an incomplete boot transaction looks like",
    );
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    let reason = "this platform publishes no boot duration simon knows how to read";

    out.push(Reading::unavailable(
        "system.boot.duration",
        Some(Unit::Seconds),
        reason,
    ));
}

/// Whether Secure Boot is enforcing, from the firmware flag.
///
/// The entity has been declared since the boot cluster was written and nothing
/// resolved it, so it fell to the unbound-id sweep and reported "no resolver
/// bound on this build" — on a machine where the flag is perfectly readable.
///
/// Two readers hold this and only one is honest. `boot_config` keeps it in a
/// `bool` and collapses a failed query with `unwrap_or(false)`, which reports
/// Secure Boot as *disabled* when it could not be read. `firmware` models it as
/// `SecureBootStatus`, which separates `Disabled` from `NotSupported` from
/// `Unknown`. That distinction is the whole reading, so `boot_config` is not
/// used here: off, unsupported and unread are three different answers and only
/// one of them is `false`.
fn resolve_secure_boot(out: &mut Vec<Reading>) {
    // Windows first, and unelevated. `firmware` asks `Confirm-SecureBootUEFI`,
    // which needs Administrator and so returns `Unknown` in normal use — this
    // machine reported "the firmware flag was not readable here" while the flag
    // sat in the registry the whole time. `SecureBoot\State\UEFISecureBootEnabled`
    // is readable without elevation and `secure_boot_enabled` already returns an
    // `Option`, so the absence survives.
    //
    // Same shape as the NVMe and ATA capabilities: both were scoped as needing
    // Administrator on a second reading and turned out not to. Check for an
    // unelevated source before accepting that a reading needs privilege.
    #[cfg(target_os = "windows")]
    {
        match crate::platform::windows::secure_boot_enabled() {
            Some(on) => {
                out.push(Reading::measured(
                    "system.boot.secure_boot",
                    serde_json::json!(on),
                    None,
                ));
            }
            None => {
                out.push(Reading::unavailable(
                    "system.boot.secure_boot",
                    None,
                    "the SecureBoot state key is absent, which is what a BIOS/CSM \
                     machine looks like — it is not the same as Secure Boot being \
                     turned off",
                ));
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        resolve_secure_boot_from_firmware(out);
    }
}

/// The non-Windows arm, split out so the `cfg` above stays readable.
#[cfg(not(target_os = "windows"))]
fn resolve_secure_boot_from_firmware(out: &mut Vec<Reading>) {
    use crate::firmware::SecureBootStatus;

    let monitor = match crate::firmware::FirmwareInventory::new() {
        Ok(m) => m,
        Err(e) => {
            out.push(Reading::unavailable(
                "system.boot.secure_boot",
                None,
                format!("the firmware inventory could not be read: {e}"),
            ));
            return;
        }
    };

    match monitor.secure_boot_status() {
        SecureBootStatus::Enabled => out.push(Reading::measured(
            "system.boot.secure_boot",
            serde_json::json!(true),
            None,
        )),
        SecureBootStatus::Disabled => out.push(Reading::measured(
            "system.boot.secure_boot",
            serde_json::json!(false),
            None,
        )),
        // Not the same as `false`. A firmware without Secure Boot cannot have it
        // turned off, and an agent auditing posture needs to tell "off" from
        // "unavailable on this hardware" before recommending anything.
        SecureBootStatus::NotSupported => out.push(Reading::unavailable(
            "system.boot.secure_boot",
            None,
            "this firmware does not implement Secure Boot, which is not the same \
             as having it disabled",
        )),
        SecureBootStatus::Unknown => out.push(Reading::unavailable(
            "system.boot.secure_boot",
            None,
            "the firmware flag was not readable here",
        )),
    }
}

/// Kernel parameters: what the platform reported, and none of what this crate
/// thinks about it.
///
/// The reader also computes `is_recommended`, `recommended`, `security_score`,
/// `network_score` and a list of free-text recommendations. Those are opinions
/// about what a value ought to be, and `simon tune`'s standing rule is that a
/// proposed value comes from what the system declared and never from this crate.
/// A score published beside a measured value borrows its authority, so the split
/// is here rather than a subset of the fields being passed through.
fn resolve_kernel_params(out: &mut Vec<Reading>) {
    let monitor = match crate::kernel_params::KernelParamsMonitor::new() {
        Ok(m) => m,
        Err(e) => {
            out.push(Reading::unavailable(
                "system.kernel_param.<none>",
                None,
                format!("kernel parameters could not be read here: {e}"),
            ));
            return;
        }
    };

    let params = &monitor.report().params;
    if params.is_empty() {
        out.push(Reading::unavailable(
            "system.kernel_param.<none>",
            None,
            "no kernel parameter was readable here; on Windows the reader knows \
             one setting and reports nothing when the query returns empty",
        ));
        return;
    }

    for (i, p) in params.iter().enumerate() {
        let base = format!("system.kernel_param.{i}");
        push_id(out, format!("{base}.name"), &p.name);
        push_id(out, format!("{base}.value"), &p.value);
        push_id(
            out,
            format!("{base}.category"),
            &format!("{:?}", p.category),
        );
    }
}

/// Services, as counts plus the names of the broken ones.
///
/// Deliberately not one entity per service. This machine runs 311, and
/// enumerating them would more than double every snapshot to carry a list no
/// consumer reads in full. The question being answered is "is anything broken,
/// and what" — so failures are named and the rest are counted.
fn resolve_services(out: &mut Vec<Reading>) {
    let monitor = match crate::services::ServiceMonitor::new() {
        Ok(m) => m,
        Err(e) => {
            out.push(Reading::unavailable(
                "system.service.<none>",
                None,
                format!("the service manager could not be reached: {e}"),
            ));
            return;
        }
    };

    let services = monitor.services();
    if services.is_empty() {
        out.push(Reading::unavailable(
            "system.service.<none>",
            None,
            "the service manager was reached and reported no services, which on \
             a running machine means the enumeration is incomplete rather than \
             that nothing is installed",
        ));
        return;
    }

    let running = services.iter().filter(|s| s.is_active()).count();
    let failed: Vec<&str> = services
        .iter()
        .filter(|s| s.is_failed())
        .map(|s| s.name.as_str())
        .collect();

    for (id, n) in [
        ("system.service.count.total", services.len()),
        ("system.service.count.running", running),
        ("system.service.count.failed", failed.len()),
    ] {
        out.push(Reading::measured(
            id,
            serde_json::json!(n),
            Some(Unit::Count),
        ));
    }

    // Nothing is pushed when nothing has failed. The count above already says
    // zero, and a `<none>` here would claim the enumeration failed.
    for (i, name) in failed.iter().enumerate() {
        push_id(out, format!("system.service.failed.{i}"), name);
    }
}

/// Print queues the spooler knows about.
fn resolve_printers(out: &mut Vec<Reading>) {
    let monitor = match crate::printer::PrinterMonitor::new() {
        Ok(m) => m,
        Err(e) => {
            out.push(Reading::unavailable(
                "system.printer.<none>",
                None,
                format!("printers could not be enumerated here: {e}"),
            ));
            return;
        }
    };

    let printers = monitor.printers();
    if printers.is_empty() {
        out.push(Reading::unavailable(
            "system.printer.<none>",
            None,
            "the spooler was reachable and holds no queue",
        ));
        return;
    }

    for (i, p) in printers.iter().enumerate() {
        let base = format!("system.printer.{i}");
        push_text(out, format!("{base}.name"), &p.name);
        push_text(out, format!("{base}.description"), &p.description);
        push_id(
            out,
            format!("{base}.connection"),
            &format!("{:?}", p.connection),
        );
        // `PrinterStatus::Unknown` renders as "unknown", which `push_id` turns
        // into an absence with a reason. That is the right outcome and it is
        // why the status goes through the guarded helper rather than being
        // pushed directly.
        push_id(out, format!("{base}.status"), &format!("{:?}", p.status));
        out.push(Reading::measured(
            format!("{base}.default"),
            serde_json::json!(p.is_default),
            None,
        ));
        push_opt(
            out,
            format!("{base}.accepting_jobs"),
            p.accepting_jobs.map(|a| serde_json::json!(a)),
            None,
            concat!(
                "whether the queue takes new work is reported by CUPS and has ",
                "no equivalent in Win32_Printer; deriving it from the printer ",
                "status is the one thing this field exists not to do"
            ),
        );
        push_opt(
            out,
            format!("{base}.color"),
            p.color.map(|c| serde_json::json!(c)),
            None,
            concat!(
                "the spooler listed no capabilities for this printer, so whether ",
                "it prints in colour was not reported",
            ),
        );
    }
}

/// Bluetooth adapters. Deliberately not the devices they can see.
fn resolve_bluetooth(out: &mut Vec<Reading>) {
    let monitor = match crate::bluetooth::BluetoothMonitor::new() {
        Ok(m) => m,
        Err(e) => {
            out.push(Reading::unavailable(
                "network.bluetooth.<none>",
                None,
                format!("Bluetooth adapters could not be enumerated here: {e}"),
            ));
            return;
        }
    };

    let adapters = monitor.adapters();
    if adapters.is_empty() {
        out.push(Reading::unavailable(
            "network.bluetooth.<none>",
            None,
            "Bluetooth enumeration succeeded and found no adapter",
        ));
        return;
    }

    for (i, a) in adapters.iter().enumerate() {
        let base = format!("network.bluetooth.{i}");
        push_text(out, format!("{base}.name"), &a.name);
        out.push(Reading::measured(
            format!("{base}.powered"),
            serde_json::json!(a.powered),
            None,
        ));
    }
}

/// What the processor says about itself, and which instructions it implements.
///
/// The reader's inferred performance scores are read and discarded here. They
/// are a table lookup keyed on the microarchitecture name, and publishing one
/// through an interface whose whole promise is that a value carries its own
/// provenance would be the clearest possible violation of that promise: there
/// is no provenance to give it. `Derived` would be a lie about the inputs and
/// `Measured` a lie about the method.
fn resolve_microarch(out: &mut Vec<Reading>) {
    let monitor = match crate::cpu_microarch::CpuMicroarchMonitor::new() {
        Ok(m) => m,
        Err(e) => {
            out.push(Reading::unavailable(
                "cpu.microarch.<none>",
                None,
                format!("the processor could not be identified here: {e}"),
            ));
            return;
        }
    };

    let report = monitor.report();
    let uarch = &report.microarch;

    // Field by field. An earlier version pushed `{:?}` of the whole struct,
    // which put a Rust debug rendering -- braces, quotes, `None` -- into a
    // reading declared as an identifier. It parsed as a value and was one only
    // in the sense that a screenshot of a table is a table.
    push_spec_id(out, "cpu.microarch.name", &uarch.name);
    push_spec_id(out, "cpu.microarch.codename", &uarch.codename);
    push_spec_id(out, "cpu.microarch.vendor", &format!("{:?}", uarch.vendor));
    push_spec_id(out, "cpu.microarch.isa", &uarch.arch);
    push_spec_opt(
        out,
        "cpu.microarch.process",
        (uarch.process_nm > 0).then(|| serde_json::json!(uarch.process_nm)),
        Some(Unit::Count),
        "simon holds no process node for this microarchitecture",
    );
    push_spec_opt(
        out,
        "cpu.microarch.year",
        (uarch.year > 0).then(|| serde_json::json!(uarch.year)),
        Some(Unit::Count),
        "simon holds no introduction date for this microarchitecture",
    );
    out.push(Reading::spec(
        "cpu.microarch.hybrid",
        serde_json::json!(uarch.hybrid),
        None,
    ));

    // Family zero identifies no x86 part that has ever shipped, so it means the
    // CPUID triple was not read rather than that this is family 0. When it is
    // missing the model and stepping beside it are missing too -- they are
    // decoded from the same leaf -- and stepping 0 is a legitimate value, so
    // publishing it while family is absent would present a default as a
    // reading. All three go together or none of them do.
    // `family > 0` was the proxy for "the triple was read", because the reader
    // returned zeros when it was not. The fields are `Option` now, so the
    // question is asked directly.
    let cpuid_read = report.family.is_some();
    for (suffix, value) in [
        ("family", report.family),
        ("model", report.model),
        ("stepping", report.stepping),
    ] {
        push_spec_opt(
            out,
            format!("cpu.microarch.{suffix}"),
            cpuid_read
                .then_some(value)
                .flatten()
                .map(|v| serde_json::json!(v)),
            Some(Unit::Count),
            "the CPUID family/model/stepping triple was not read on this platform",
        );
    }
    for (suffix, value) in [
        ("physical_cores", report.physical_cores),
        ("logical_cores", report.logical_cores),
    ] {
        out.push(Reading::spec(
            format!("cpu.microarch.{suffix}"),
            serde_json::json!(value),
            Some(Unit::Count),
        ));
    }
    out.push(Reading::measured(
        "cpu.microarch.smt_enabled",
        serde_json::json!(report.smt_enabled),
        None,
    ));

    // Supported only. An extension the processor does not implement is not a
    // property of this machine, and listing it with a false flag invites a
    // consumer that forgets to read the flag to conclude the opposite.
    let supported: Vec<_> = report.extensions.iter().filter(|x| x.supported).collect();
    if supported.is_empty() {
        out.push(Reading::unavailable(
            "cpu.microarch.extension.<none>",
            None,
            "the processor was identified and reported no instruction set extensions, \
             which means the extension probe failed rather than that the CPU has none",
        ));
        return;
    }
    for (i, x) in supported.iter().enumerate() {
        let base = format!("cpu.microarch.extension.{i}");
        push_spec_id(out, format!("{base}.name"), &x.name);
        push_spec_id(
            out,
            format!("{base}.category"),
            &format!("{:?}", x.category),
        );
        push_spec_text(out, format!("{base}.description"), &x.description);
    }
}

/// Hardware cryptographic acceleration and random sources.
///
/// Both lists are filtered to what is actually present, and both carry their own
/// `<none>` row. A machine with no hardware RNG is a real state an agent may
/// need to act on -- it changes how a key should be generated -- and it must not
/// be reachable by the same silence as a reader that failed.
fn resolve_crypto(out: &mut Vec<Reading>) {
    let monitor = match crate::crypto_accel::CryptoAccelMonitor::new() {
        Ok(m) => m,
        Err(e) => {
            out.push(Reading::unavailable(
                "cpu.crypto.<none>",
                None,
                format!("hardware crypto support could not be determined here: {e}"),
            ));
            return;
        }
    };

    let report = monitor.report();

    let accelerated: Vec<_> = report
        .features
        .iter()
        .filter(|f| f.hardware_accelerated)
        .collect();
    if accelerated.is_empty() {
        out.push(Reading::unavailable(
            "cpu.crypto.<none>",
            None,
            "the processor reports no hardware-accelerated cryptographic primitive",
        ));
    }
    for (i, f) in accelerated.iter().enumerate() {
        let base = format!("cpu.crypto.feature.{i}");
        push_spec_id(out, format!("{base}.name"), &f.name);
        push_spec_id(out, format!("{base}.flag"), &f.cpu_flag);
        push_spec_id(
            out,
            format!("{base}.category"),
            &format!("{:?}", f.category),
        );
        // GB/s to bytes per second. Decimal, matching how the constants were
        // written down; and `Derived`, because a constant is not a measurement
        // however plausible the number looks.
        match f.estimated_throughput_gbs {
            Some(gbs) => out.push(Reading::derived(
                format!("{base}.throughput"),
                serde_json::json!(gbs * 1_000_000_000.0),
                Some(Unit::BytesPerSecond),
            )),
            None => out.push(Reading::unavailable(
                format!("{base}.throughput"),
                Some(Unit::BytesPerSecond),
                "simon holds no throughput estimate for this primitive",
            )),
        }
    }

    let sources: Vec<_> = report.rng_sources.iter().filter(|r| r.available).collect();
    if sources.is_empty() {
        // Two very different situations reach here, and saying "this machine
        // has no hardware RNG" would be wrong in one of them. On Windows the
        // feature list above routinely carries RDRAND and RDSEED while
        // `rng_sources` comes back empty, because the two are filled by
        // different probes and only one of them is implemented there. Asserting
        // the machine has no random source while simon has just reported the
        // instruction that provides one is a contradiction an agent would be
        // right to act on and wrong to believe.
        let instruction_present = accelerated.iter().any(|f| {
            matches!(
                f.category,
                crate::crypto_accel::CryptoCategory::RandomNumberGen
            )
        });
        out.push(Reading::unavailable(
            "cpu.crypto.rng.<none>",
            None,
            if instruction_present {
                "the hardware random source probe enumerated nothing, but the feature \
                 list above reports a random number instruction - this is a gap in the \
                 probe on this platform rather than a machine without an RNG"
            } else {
                "no hardware random source was found, and no random number instruction \
                 was reported either"
            },
        ));
        return;
    }
    for (i, r) in sources.iter().enumerate() {
        let base = format!("cpu.crypto.rng.{i}");
        push_spec_id(out, format!("{base}.name"), &r.name);
        push_spec_id(
            out,
            format!("{base}.source"),
            &format!("{:?}", r.source_type),
        );
        push_spec_opt(
            out,
            format!("{base}.quality"),
            r.quality.map(|q| serde_json::json!(q)),
            Some(Unit::Count),
            "this source declares no entropy figure, and assuming one would be worse \
             than saying nothing",
        );
    }
}

/// Memory bandwidth, every figure of which is arithmetic.
///
/// The estimate is worth publishing and worth labelling. `Derived` is not a
/// weaker `Measured`; it is a different claim, and the difference is the whole
/// reason an agent can use these numbers safely. Nothing here was benchmarked.
fn resolve_memory_bandwidth(out: &mut Vec<Reading>) {
    let monitor = match crate::memory_bandwidth::MemoryBandwidthMonitor::new() {
        Ok(m) => m,
        Err(e) => {
            out.push(Reading::unavailable(
                "memory.bandwidth.<none>",
                None,
                format!(
                    "the memory configuration needed for an estimate is not readable here: {e}"
                ),
            ));
            return;
        }
    };

    let est = monitor.estimate();

    push_spec_id(
        out,
        "memory.bandwidth.generation",
        &format!("{:?}", est.generation),
    );

    // Everything below rests on the generation, and an unidentified generation
    // does not stop the estimator producing numbers -- it falls back to 3200
    // MT/s, a 64-bit bus and a 0.75 efficiency factor, none of which is a fact
    // about this machine. A VM's SMBIOS routinely names no generation, so on
    // those the whole chain was publishing defaults as `Specification` and
    // `Derived` readings with nothing to distinguish them from figures that had
    // been read. Withholding the chain is the only honest answer: the inputs
    // were not read, so the outputs are not estimates of anything.
    let unidentified = est.generation == crate::memory_bandwidth::MemoryGeneration::Unknown;
    const NO_GENERATION: &str = "the memory generation was not identified, so the transfer \
                                 rate and every bandwidth figure below it would be a built-in \
                                 default rather than a property of this machine";

    if unidentified {
        for id in [
            "memory.bandwidth.speed",
            "memory.bandwidth.peak",
            "memory.bandwidth.achievable",
            "memory.bandwidth.stream_triad",
        ] {
            let unit = if id == "memory.bandwidth.speed" {
                Unit::Count
            } else {
                Unit::BytesPerSecond
            };
            out.push(Reading::unavailable(id, Some(unit), NO_GENERATION));
        }
        out.push(Reading::spec(
            "memory.bandwidth.channels",
            serde_json::json!(est.channels.active_channels),
            Some(Unit::Count),
        ));
        out.push(Reading::spec(
            "memory.bandwidth.max_channels",
            serde_json::json!(est.channels.max_channels),
            Some(Unit::Count),
        ));
        return;
    }

    out.push(Reading::spec(
        "memory.bandwidth.speed",
        serde_json::json!(est.speed_mts),
        Some(Unit::Count),
    ));
    out.push(Reading::spec(
        "memory.bandwidth.channels",
        serde_json::json!(est.channels.active_channels),
        Some(Unit::Count),
    ));
    out.push(Reading::spec(
        "memory.bandwidth.max_channels",
        serde_json::json!(est.channels.max_channels),
        Some(Unit::Count),
    ));

    // GB/s to bytes per second, which is the unit the entities declare. Decimal
    // gigabytes, not gibibytes: memory bandwidth is quoted in the same decimal
    // units as the transfer rate it is computed from, and converting with 1024
    // would inflate every figure by 7%.
    const GB: f64 = 1_000_000_000.0;
    out.push(Reading::derived(
        "memory.bandwidth.peak",
        serde_json::json!(est.peak_bandwidth_gbs * GB),
        Some(Unit::BytesPerSecond),
    ));
    out.push(Reading::derived(
        "memory.bandwidth.achievable",
        serde_json::json!(est.achievable_bandwidth_gbs * GB),
        Some(Unit::BytesPerSecond),
    ));
    out.push(Reading::derived(
        "memory.bandwidth.stream_triad",
        serde_json::json!(est.stream_triad_estimate_gbs * GB),
        Some(Unit::BytesPerSecond),
    ));
}

/// `push_opt` for firmware-declared values: same absence handling, but the
/// present case carries `Specification` rather than `Measured`. Without this a
/// DIMM's part number would claim to have been measured off the module.
fn push_spec_opt(
    out: &mut Vec<Reading>,
    id: impl Into<String>,
    value: Option<serde_json::Value>,
    unit: Option<Unit>,
    why_absent: &str,
) {
    match value {
        Some(v) if v.as_str().is_some_and(names_an_absence) => {
            out.push(Reading::unavailable(
                id,
                unit,
                format!("reader returned {v}, which names an absence rather than a value"),
            ));
        }
        Some(v) => out.push(Reading::spec(id, v, unit)),
        None => out.push(Reading::unavailable(id, unit, why_absent)),
    }
}

/// Package energy domains, where the platform exposes them.
///
/// The absence path is written first and deliberately: on Windows and macOS
/// there is no unprivileged RAPL interface at all, and the failure has to arrive
/// as a stated reason rather than as an empty list. The reader returned
/// `Ok(vec![])` on both until this was wired, which is indistinguishable from a
/// Linux box whose zones are all disabled.
fn resolve_rapl(out: &mut Vec<Reading>) {
    let mut monitor = match crate::rapl::RaplMonitor::new() {
        Ok(m) => m,
        Err(e) => {
            out.push(Reading::unavailable(
                "power.rapl.<none>",
                None,
                format!("RAPL is not readable here: {e}"),
            ));
            return;
        }
    };

    if let Err(e) = monitor.refresh() {
        out.push(Reading::unavailable(
            "power.rapl.<none>",
            None,
            format!("RAPL is not readable here: {e}"),
        ));
        return;
    }

    let readings = monitor.readings();

    if readings.is_empty() {
        // Reached only where the interface exists and enumerated nothing, which
        // is a different fact from the platform not having one.
        out.push(Reading::unavailable(
            "power.rapl.<none>",
            None,
            "the RAPL interface is present and enumerated no energy domains",
        ));
        return;
    }

    for (i, r) in readings.iter().enumerate() {
        let base = format!("power.rapl.{i}");
        push_text(out, format!("{base}.name"), &r.name);
        out.push(Reading::measured(
            format!("{base}.energy"),
            serde_json::json!(r.energy_uj),
            Some(Unit::Count),
        ));
        out.push(Reading::measured(
            format!("{base}.max_energy_range"),
            serde_json::json!(r.max_energy_range_uj),
            Some(Unit::Count),
        ));
        push_opt(
            out,
            format!("{base}.power_limit"),
            // Microwatts to watts, which is the unit the entity declares.
            r.power_limit_uw
                .map(|uw| serde_json::json!(uw as f64 / 1_000_000.0)),
            Some(Unit::Watts),
            "this RAPL domain publishes no power constraint",
        );
        out.push(Reading::measured(
            format!("{base}.enabled"),
            serde_json::json!(r.enabled),
            None,
        ));
    }
}

fn resolve_power(out: &mut Vec<Reading>) {
    match crate::battery::BatteryMonitor::new() {
        Ok(monitor) => {
            let batteries = monitor.batteries();
            if batteries.is_empty() {
                out.push(Reading::unavailable(
                    "power.battery.percentage",
                    Some(Unit::Percent),
                    "no battery present (desktop or always-AC machine)",
                ));
            } else {
                push_opt(
                    out,
                    "power.battery.percentage".to_string(),
                    batteries[0].charge_percent.map(|p| serde_json::json!(p)),
                    Some(Unit::Percent),
                    "a battery is present but its charge gauge could not be read",
                );
            }
        }
        Err(e) => out.push(Reading::unavailable(
            "power.battery.percentage",
            Some(Unit::Percent),
            format!("battery monitor unavailable: {e}"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The invariant the whole module exists for: a missing value is never a zero,
    /// and never silent about why.
    #[test]
    fn unavailable_readings_carry_no_value_and_state_a_reason() {
        for r in snapshot() {
            if r.provenance == Provenance::Unavailable {
                assert!(
                    r.value.is_none(),
                    "{} is unavailable but carries a value: {:?}",
                    r.id,
                    r.value
                );
                assert!(
                    r.note.as_ref().is_some_and(|n| !n.trim().is_empty()),
                    "{} is unavailable with no reason given",
                    r.id
                );
            } else {
                assert!(
                    r.value.is_some(),
                    "{} claims provenance {:?} but has no value",
                    r.id,
                    r.provenance
                );
            }
        }
    }

    #[test]
    fn only_measured_readings_count_as_observations() {
        for r in snapshot() {
            let expected = r.provenance == Provenance::Measured && r.value.is_some();
            assert_eq!(r.is_observation(), expected, "{}", r.id);
        }
    }

    /// Every resolved id must be one the ontology names — otherwise the schema an
    /// agent fetched does not describe what it receives.
    #[test]
    fn every_reading_maps_to_a_declared_entity() {
        let ontology = Ontology::build();
        for r in snapshot() {
            assert!(
                lookup_template(&ontology, &r.id).is_some(),
                "{} was resolved but is not declared in the ontology",
                r.id
            );
            assert!(
                !Ontology::is_template(&r.id),
                "{} is a template, not a concrete reading",
                r.id
            );
        }
    }

    /// Live values must satisfy the ranges the ontology declares.
    ///
    /// This held at rest and failed under the load of the full test suite, when a
    /// per-core utilization derived as `user + system` drifted past 100 percent.
    /// Two things changed: the derivation now uses `100 - idle`, and `snapshot`
    /// withholds anything the ontology rejects. The assertion is therefore now
    /// structural rather than a matter of timing.
    #[test]
    fn live_readings_are_physically_possible() {
        let problems = validate(&snapshot());
        assert!(problems.is_empty(), "impossible readings: {problems:#?}");
    }

    /// The gate must withhold rather than clamp. A clamped value looks like a
    /// reading and is not one, which is the failure mode this module exists to
    /// prevent — and it would destroy the evidence that sampling went wrong.
    #[test]
    fn out_of_range_values_are_withheld_with_the_offending_value_named() {
        use crate::ontology::{Domain, EntityKind};

        let ontology = Ontology::build();
        let entity = ontology.get("cpu.total.utilization").unwrap();
        assert_eq!(entity.domain, Domain::Cpu);
        assert_eq!(entity.kind, EntityKind::Measurement);

        // The range check the gate relies on, exercised directly: an impossible
        // percentage must be reported as a problem, not silently corrected.
        let problem = entity
            .validate_range(103.0)
            .expect("103 percent should be rejected");
        assert!(
            problem.contains("103"),
            "the rejection must name the offending value so it can be diagnosed, \
             got: {problem}"
        );
        assert!(entity.validate_range(99.9).is_none());
    }

    /// An id an agent cannot type is not an id.
    ///
    /// Device names come from vendors, not from simon: the first version of the
    /// network resolver emitted `network.Bluetooth Network Connection.rx_bytes`,
    /// which cannot be passed to `simon get` without quoting and whose spaces make
    /// the dotted structure ambiguous. Segments are sanitised at construction; this
    /// asserts the whole snapshot honours it.
    #[test]
    fn every_id_is_shell_safe_and_well_structured() {
        for r in snapshot() {
            assert!(
                !r.id.contains(char::is_whitespace),
                "{} contains whitespace, so it cannot be passed as a bare argument",
                r.id
            );
            assert!(
                !r.id.contains("..") && !r.id.starts_with('.') && !r.id.ends_with('.'),
                "malformed id: {}",
                r.id
            );
            for segment in r.id.split('.') {
                assert!(!segment.is_empty(), "{} has an empty path segment", r.id);
            }
        }
    }

    #[test]
    fn id_segments_are_sanitised_without_collapsing_distinct_names() {
        assert_eq!(
            id_segment("Bluetooth Network Connection"),
            "Bluetooth_Network_Connection"
        );
        assert_eq!(id_segment("eth0.100"), "eth0_100");
        assert_eq!(id_segment("  spaced  "), "spaced");
        // A name made entirely of separators still has to yield a usable segment.
        assert_eq!(id_segment("///"), "unnamed");
        // Distinct names must not collide into one id.
        assert_ne!(id_segment("eth0"), id_segment("eth1"));
    }

    /// A diagnostic must only appear when its condition holds. `disk.<none>` in a
    /// snapshot that also lists eight disk readings is a contradiction, and one that
    /// would teach an agent to distrust the diagnostics generally.
    #[test]
    fn diagnostics_do_not_contradict_the_readings_beside_them() {
        let readings = snapshot();
        for domain in crate::ontology::Domain::ALL {
            let none_id = format!("{}.<none>", domain.as_str());
            let has_none = readings.iter().any(|r| r.id == none_id);
            if !has_none {
                continue;
            }
            // Only rows carrying an actual observation can contradict the
            // diagnostic. An unavailable row asserts nothing about the hardware:
            // `gpu.setting.perf_level` reported as "no resolver bound on this
            // build" sits perfectly happily beside `gpu.<none>`, because neither
            // claims a GPU is present. Counting those made the test fire on a
            // Linux runner with no GPU, where three writable GPU settings are
            // declared from their registered apply handlers and read by nothing.
            //
            // The doc above always said "eight disk readings" — readings, values.
            // The predicate simply did not say it.
            let real_rows: Vec<&str> = readings
                .iter()
                .filter(|r| {
                    r.id.starts_with(&format!("{}.", domain.as_str()))
                        && !r.id.contains('<')
                        && r.is_observation()
                })
                .map(|r| r.id.as_str())
                .collect();
            // The ids, not just how many. A count says a contradiction exists and
            // leaves you to guess which resolver caused it; on a platform you
            // cannot reproduce locally that guess is the whole cost of the fix.
            assert!(
                real_rows.is_empty(),
                "{none_id} claims the domain enumerated nothing, but these {} rows are present alongside it: {real_rows:?}",
                real_rows.len(),
            );
        }
    }

    /// Every domain the ontology declares must appear in a snapshot, even when the
    /// machine has no such device. Silence would leave an agent unable to tell "no
    /// disks here" from "simon does not read disks".
    #[test]
    fn no_declared_domain_is_silently_absent() {
        let readings = snapshot();
        for domain in crate::ontology::Domain::ALL {
            let prefix = format!("{}.", domain.as_str());
            assert!(
                readings.iter().any(|r| r.id.starts_with(&prefix)),
                "domain {} produced no rows at all — an agent cannot distinguish an \
                 absent device from an unimplemented reader",
                domain.as_str()
            );
        }
    }

    #[test]
    fn ids_are_unique_within_a_snapshot() {
        let readings = snapshot();
        let mut seen = std::collections::HashSet::new();
        for r in &readings {
            assert!(seen.insert(r.id.clone()), "duplicate reading id: {}", r.id);
        }
    }

    #[test]
    fn get_rejects_templates_and_finds_concrete_ids() {
        assert!(
            get("gpu.{n}.name").is_none(),
            "a template is not a question with an answer"
        );
        // memory.total resolves on every supported platform; on an unsupported one
        // it is still present, as an unavailable reading.
        assert!(get("memory.total").is_some());
        assert!(get("no.such.entity").is_none());
    }

    #[test]
    fn coverage_accounts_for_every_reading() {
        // One snapshot, tallied. The previous version compared `coverage()`
        // against a second, independent `snapshot()` and asserted equal lengths —
        // which is a race on a live machine, since a USB device or a process can
        // come and go between the two calls. It passed for as long as the suite
        // was quiet enough, then failed under parallel load for no reason
        // connected to what it was testing.
        let readings = snapshot();
        let c = coverage_of(&readings);
        assert_eq!(c.total, c.resolved + c.unavailable);
        assert_eq!(c.total, readings.len());
    }
}
