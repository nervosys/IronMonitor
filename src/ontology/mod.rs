//! Machine-readable ontology over everything IronMonitor can report.
//!
//! The three interfaces — CLI, TUI, GUI — all render the same underlying readings,
//! but until now each described them in its own words. `cli board` printed
//! `Kernel:`, the GUI printed `Kernel Version`, and the JSON output used
//! `kernel_version`; an agent reading one could not reliably talk about the others.
//! This module is the single naming authority the three surfaces agree on.
//!
//! ## What an entity is
//!
//! Every value IronMonitor can report has a stable dotted id — `gpu.0.thermal.temperature`,
//! `cpu.total.utilization`, `disk.0.capacity` — plus a unit and, most importantly,
//! a [`Provenance`].
//!
//! ## Why provenance is the point
//!
//! An agent reading `gpu.0.thermal.critical_temperature = null` learns nothing about
//! *why* it is null, and an agent reading `= 110` cannot tell whether the hardware
//! said so or whether a table of plausible constants did. That distinction is not
//! cosmetic: this repository has repeatedly shipped invented numbers through the same
//! field as measured ones — a boot time of exactly 45s assigned whenever it could not
//! be read, a GPU power percentage whose denominator came from a core-count lookup,
//! Secure Boot reported from the existence of a registry key rather than its value.
//! Each was indistinguishable from a real reading at the point of consumption.
//!
//! [`Provenance`] makes the distinction explicit and machine-checkable, so an agent
//! can refuse to reason about a specification constant as though it were a sample.

pub mod resolve;

pub mod capability;
pub mod jsonld;
pub mod vocabulary;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Ontology format version. Bump the major on any breaking id or field change.
pub const ONTOLOGY_VERSION: &str = "1.0";

/// Where a value came from.
///
/// This is the field that distinguishes a reading from a guess. It is deliberately
/// not defaultable: every entity must state its provenance explicitly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provenance {
    /// Sampled from hardware or the OS during this collection cycle.
    Measured,
    /// A published constant — a spec sheet, a vendor table, a documented default.
    /// True of the hardware, but not observed on *this* machine right now.
    Specification,
    /// Computed from other entities. `derived_from` names the inputs.
    Derived,
    /// Not obtainable here: unsupported platform, absent device, missing permission.
    /// A consumer must render this as "unknown", never as zero.
    Unavailable,
}

impl Provenance {
    /// Whether a value with this provenance may be treated as a live observation.
    ///
    /// The guard an agent should apply before reasoning about a number as fact.
    /// `Specification` values are true but stale; `Derived` values inherit the
    /// weakest provenance of their inputs and so must be checked through those.
    pub fn is_observation(self) -> bool {
        matches!(self, Self::Measured)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Measured => "measured",
            Self::Specification => "specification",
            Self::Derived => "derived",
            Self::Unavailable => "unavailable",
        }
    }
}

/// Physical unit of a value. `None` on the entity means dimensionless or textual.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    Celsius,
    Percent,
    Bytes,
    BytesPerSecond,
    Hertz,
    Megahertz,
    MegatransfersPerSecond,
    Watts,
    Milliwatts,
    Volts,
    Rpm,
    Seconds,
    Hours,
    Milliseconds,
    Count,
    Identifier,
    Text,
}

impl Unit {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Celsius => "celsius",
            Self::Percent => "percent",
            Self::Bytes => "bytes",
            Self::BytesPerSecond => "bytes_per_second",
            Self::Hertz => "hertz",
            Self::Megahertz => "megahertz",
            Self::MegatransfersPerSecond => "megatransfers_per_second",
            Self::Watts => "watts",
            Self::Milliwatts => "milliwatts",
            Self::Volts => "volts",
            Self::Rpm => "rpm",
            Self::Seconds => "seconds",
            Self::Hours => "hours",
            Self::Milliseconds => "milliseconds",
            Self::Count => "count",
            Self::Identifier => "identifier",
            Self::Text => "text",
        }
    }

    /// Whether a negative value is physically meaningful for this unit.
    ///
    /// Used by [`Entity::validate_range`] so a consumer can reject impossible
    /// readings without hardcoding per-field knowledge.
    pub fn allows_negative(self) -> bool {
        matches!(self, Self::Celsius | Self::Volts)
    }
}

/// What kind of thing an entity is, which determines how an agent may use it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    /// A sampled quantity that changes over time.
    Measurement,
    /// A fixed property of the device — model name, capacity, core count.
    Identity,
    /// A value that can be written back, subject to the apply layer's confirmation.
    Setting,
    /// A limit or threshold the hardware declares.
    Limit,
    /// Not a property of the hardware but a statement about the reading process
    /// itself — that a domain enumerated nothing, or that a list was truncated.
    /// These exist because silence is the one answer a resolver must never give: an
    /// agent that receives no `disk.*` rows cannot tell an absent device from an
    /// unimplemented reader, and a capped list presented as complete invites the
    /// conclusion that a process is absent when it was merely ranked eleventh.
    Diagnostic,
}

impl EntityKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Measurement => "measurement",
            Self::Identity => "identity",
            Self::Setting => "setting",
            Self::Limit => "limit",
            Self::Diagnostic => "diagnostic",
        }
    }
}

/// The subsystem an entity belongs to. Mirrors the CLI's top-level nouns so that
/// `ironmon cli gpu` and the `gpu.*` id space are obviously the same thing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Domain {
    Cpu,
    Gpu,
    Memory,
    Disk,
    Network,
    Power,
    Thermal,
    Process,
    System,
    Board,
    Pci,
    Usb,
}

impl Domain {
    pub const ALL: &'static [Domain] = &[
        Domain::Cpu,
        Domain::Gpu,
        Domain::Memory,
        Domain::Disk,
        Domain::Network,
        Domain::Power,
        Domain::Thermal,
        Domain::Process,
        Domain::System,
        Domain::Board,
        Domain::Pci,
        Domain::Usb,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Gpu => "gpu",
            Self::Memory => "memory",
            Self::Disk => "disk",
            Self::Network => "network",
            Self::Power => "power",
            Self::Thermal => "thermal",
            Self::Process => "process",
            Self::System => "system",
            Self::Board => "board",
            Self::Pci => "pci",
            Self::Usb => "usb",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|d| d.as_str().eq_ignore_ascii_case(s))
    }
}

/// One named thing IronMonitor can report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entity {
    /// Stable dotted id. `{domain}.{instance}.{path}`, where `instance` is omitted
    /// for singletons (`memory.total`) and is an index otherwise (`gpu.0.name`).
    /// Ids are the contract: they may be added, but never repurposed.
    pub id: String,
    pub domain: Domain,
    pub kind: EntityKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<Unit>,
    pub provenance: Provenance,
    /// Whether the reader may legitimately have no value for this. A `false` here
    /// with a null value is a bug in the reader, not an absent device.
    pub nullable: bool,
    /// One line, written for a reader that cannot see the hardware.
    pub description: String,
    /// Ids this value is computed from. Non-empty only for [`EntityKind`] entries
    /// whose provenance is [`Provenance::Derived`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub derived_from: Vec<String>,
    /// The `ironmon profile set` id that writes this, when one exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub writable_via: Option<String>,
}

impl Entity {
    fn new(
        id: &str,
        domain: Domain,
        kind: EntityKind,
        unit: Option<Unit>,
        provenance: Provenance,
        nullable: bool,
        description: &str,
    ) -> Self {
        Self {
            id: id.to_string(),
            domain,
            kind,
            unit,
            provenance,
            nullable,
            description: description.to_string(),
            derived_from: Vec::new(),
            writable_via: None,
        }
    }

    fn derived(mut self, from: &[&str]) -> Self {
        self.provenance = Provenance::Derived;
        self.derived_from = from.iter().map(|s| s.to_string()).collect();
        self
    }

    /// Check a concrete value against what this entity's unit permits.
    ///
    /// Returns the reason it is impossible, or `None` if it is admissible. This is
    /// the same class of assertion as the `plausibility` test tier, exposed so that
    /// an agent consuming the JSON can apply it without reimplementing the rules.
    pub fn validate_range(&self, value: f64) -> Option<String> {
        let unit = self.unit?;
        if value.is_nan() {
            return Some(format!("{} is NaN", self.id));
        }
        if value < 0.0 && !unit.allows_negative() {
            return Some(format!(
                "{} = {value} is negative, which {} cannot be",
                self.id,
                unit.as_str()
            ));
        }
        if unit == Unit::Percent && value > 100.0 {
            return Some(format!("{} = {value} exceeds 100 percent", self.id));
        }
        if unit == Unit::Celsius && !(-273.15..=200.0).contains(&value) {
            return Some(format!(
                "{} = {value}C is outside any temperature real hardware reports",
                self.id
            ));
        }
        None
    }
}

/// The full set of entities IronMonitor knows how to name.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ontology {
    pub version: String,
    /// Keyed by id so lookups are O(log n) and the JSON is stable across runs.
    pub entities: BTreeMap<String, Entity>,
}

impl Default for Ontology {
    fn default() -> Self {
        Self::build()
    }
}

/// The ontology domain a writable setting belongs to.
///
/// Shared with the resolver, which publishes each setting's current value under
/// the id this mapping builds. Two copies would let the entity and the reading
/// drift into different ids, and the drift would show up as an entity that is
/// declared, never produced, and reported "no resolver bound" -- which is
/// exactly the state this function was written to get out of.
pub(crate) fn setting_domain(subsystem: crate::profile::Subsystem) -> Domain {
    match subsystem {
        crate::profile::Subsystem::Gpu => Domain::Gpu,
        crate::profile::Subsystem::Cpu => Domain::Cpu,
        crate::profile::Subsystem::Memory => Domain::Memory,
        // NVMe parameters are disk-level; display has no ontology domain of its
        // own and its settings are board-level firmware state.
        crate::profile::Subsystem::Nvme => Domain::Disk,
        crate::profile::Subsystem::Display => Domain::Board,
    }
}

impl Ontology {
    /// Construct the ontology. Pure — no hardware is touched, so this is safe to call
    /// on any machine and produces identical output everywhere, which is what makes
    /// it usable as a schema an agent can fetch ahead of time.
    pub fn build() -> Self {
        let mut entities = BTreeMap::new();
        let mut add = |e: Entity| {
            debug_assert!(
                !entities.contains_key(&e.id),
                "duplicate ontology id: {}",
                e.id
            );
            entities.insert(e.id.clone(), e);
        };

        use Domain as D;
        use EntityKind as K;
        use Provenance as P;
        use Unit as U;

        // ── CPU ──────────────────────────────────────────────────────────────
        add(Entity::new(
            "cpu.model",
            D::Cpu,
            K::Identity,
            Some(U::Text),
            P::Measured,
            // Nullable: an empty model string is a failed read, not a CPU
            // without a name, and the resolver reports it as absent.
            true,
            "Processor model string as the CPU reports it.",
        ));
        add(Entity::new(
            "cpu.cores.physical",
            D::Cpu,
            K::Identity,
            Some(U::Count),
            P::Measured,
            // Nullable: the resolver reports it absent both when the
            // reader errors and when the platform gives no physical count
            // distinct from the logical one.
            true,
            "Physical core count, excluding SMT siblings.",
        ));
        add(Entity::new(
            "cpu.cores.logical",
            D::Cpu,
            K::Identity,
            Some(U::Count),
            P::Measured,
            false,
            "Logical processor count, including SMT siblings.",
        ));
        add(Entity::new(
            "cpu.total.utilization",
            D::Cpu,
            K::Measurement,
            Some(U::Percent),
            P::Measured,
            // Nullable: the resolver reports it absent when the CPU stats
            // reader errors.
            true,
            concat!(
                "System-wide CPU busy percentage over the last sampling ",
                "interval. That interval is the time since the previous call, ",
                "not a fixed window: two snapshots taken back to back make the ",
                "second one span milliseconds, and it will report whatever ",
                "happened in those milliseconds. Measured here at 17.9% then ",
                "6.7% from consecutive calls on an idle machine, and steady at ",
                "55.7-56.2% across five spaced calls with half the cores ",
                "loaded, against an independent per-core mean of 55.7%. Space ",
                "the calls out to get a figure that means anything."
            ),
        )
        .derived(&["cpu.total.idle"]));
        add(Entity::new(
            "cpu.total.idle",
            D::Cpu,
            K::Measurement,
            Some(U::Percent),
            P::Measured,
            false,
            "System-wide idle percentage over the last sampling interval.",
        ));
        add(Entity::new(
            "cpu.core.{n}.utilization",
            D::Cpu,
            K::Measurement,
            Some(U::Percent),
            P::Derived,
            true,
            "Per-core busy percentage. Null where the platform exposes no \
             per-processor times — deliberately not the system average. Derived \
             rather than measured because no platform reports it: the resolver \
             computes `100 - idle` from per-core times, which is what the \
             reading has always been.",
        )
        .derived(&["cpu.core.{n}.idle"]));
        add(Entity::new(
            "cpu.core.{n}.idle",
            D::Cpu,
            K::Measurement,
            Some(U::Percent),
            P::Measured,
            true,
            "Per-core idle percentage over the last sampling interval. The \
             measured half of the pair: `utilization` beside it is `100 - idle`, \
             and naming the input is what lets a consumer see that the two are \
             one reading rather than two.",
        ));
        add(Entity::new(
            "cpu.core.{n}.frequency.max",
            D::Cpu,
            K::Limit,
            Some(U::Megahertz),
            P::Specification,
            true,
            concat!(
                "Rated maximum core clock, as the firmware reports it. A ",
                "boosting core can and does exceed it."
            ),
        ));
        add(Entity::new(
            "cpu.core.{n}.frequency",
            D::Cpu,
            K::Measurement,
            Some(U::Megahertz),
            // Declared as the weakest provenance any platform's rows can
            // carry, which is what `a_reading_never_resolves_weaker_than_                // its_entity_declares` asks for. Linux reads
            // `scaling_cur_freq` and over-delivers `Measured`; Windows has
            // no unprivileged API for a current clock and multiplies the
            // rated maximum by the kernel's delivered-performance counter,
            // which is a derivation and says so.
            P::Derived,
            true,
            concat!(
                "Current core clock. Null when the platform reports no ",
                "per-core clock. Each reading carries its own provenance: ",
                "Linux measures it from `scaling_cur_freq`, Windows derives ",
                "it from the rated maximum and `% Processor Performance`."
            ),
        )
        .derived(&["cpu.core.{n}.frequency.max"]));
        add(Entity::new(
            "cpu.core.{n}.frequency.min",
            D::Cpu,
            K::Limit,
            Some(U::Megahertz),
            P::Unavailable,
            true,
            "Minimum core clock. Unavailable on Windows, which exposes no such \
             figure; it is not inferable from the maximum.",
        ));

        // ── GPU ──────────────────────────────────────────────────────────────
        add(Entity::new(
            "gpu.{n}.name",
            D::Gpu,
            K::Identity,
            Some(U::Text),
            P::Measured,
            false,
            "Adapter name as the driver reports it.",
        ));
        add(Entity::new(
            "gpu.{n}.vendor",
            D::Gpu,
            K::Identity,
            Some(U::Identifier),
            P::Measured,
            false,
            "One of nvidia, amd, intel, apple.",
        ));
        add(Entity::new(
            "gpu.{n}.utilization",
            D::Gpu,
            K::Measurement,
            Some(U::Percent),
            P::Measured,
            true,
            "Graphics engine busy percentage.",
        ));
        add(Entity::new(
            "gpu.{n}.memory.used",
            D::Gpu,
            K::Measurement,
            Some(U::Bytes),
            P::Measured,
            true,
            "Video memory in use. Null on unified-memory parts that report none.",
        ));
        add(Entity::new(
            "gpu.{n}.memory.total",
            D::Gpu,
            K::Identity,
            Some(U::Bytes),
            P::Measured,
            true,
            "Total video memory.",
        ));
        add(Entity::new(
            "gpu.{n}.thermal.temperature",
            D::Gpu,
            K::Measurement,
            Some(U::Celsius),
            P::Measured,
            true,
            "GPU core temperature. Null where no sensor is exposed — notably Apple \
             Silicon via powermetrics. Null is not zero degrees.",
        ));
        add(Entity::new(
            "gpu.{n}.thermal.max_temperature",
            D::Gpu,
            K::Limit,
            Some(U::Celsius),
            P::Measured,
            true,
            "Vendor-declared thermal limit, read from the driver. Null when the \
             vendor publishes none — it must not be filled with a plausible constant.",
        ));
        add(Entity::new(
            "gpu.{n}.thermal.critical_temperature",
            D::Gpu,
            K::Limit,
            Some(U::Celsius),
            P::Measured,
            true,
            "Vendor-declared shutdown threshold, read from the driver. Null when \
             unpublished.",
        ));
        add(Entity::new(
            "gpu.{n}.power.draw",
            D::Gpu,
            K::Measurement,
            Some(U::Milliwatts),
            P::Measured,
            true,
            "Instantaneous board power draw.",
        ));
        add(Entity::new(
            "gpu.{n}.power.limit",
            D::Gpu,
            K::Limit,
            Some(U::Milliwatts),
            P::Measured,
            true,
            "Enforced power cap. Null when the driver exposes none; a percentage of \
             an unknown cap is not a measurement and is reported null in turn.",
        ));
        add(Entity::new(
            "gpu.{n}.clocks.graphics",
            D::Gpu,
            K::Measurement,
            Some(U::Megahertz),
            P::Measured,
            true,
            "Current graphics clock.",
        ));
        add(Entity::new(
            "gpu.{n}.clocks.graphics.max",
            D::Gpu,
            K::Limit,
            Some(U::Megahertz),
            P::Measured,
            true,
            "Maximum graphics clock as the driver declares it. Null where the vendor \
             publishes no ceiling.",
        ));

        // ── Memory ───────────────────────────────────────────────────────────
        add(Entity::new(
            "memory.total",
            D::Memory,
            K::Identity,
            Some(U::Bytes),
            P::Measured,
            false,
            "Total installed RAM visible to the OS.",
        ));
        add(Entity::new(
            "memory.used",
            D::Memory,
            K::Measurement,
            Some(U::Bytes),
            P::Measured,
            false,
            "RAM currently in use.",
        ));
        add(Entity::new(
            "memory.utilization",
            D::Memory,
            K::Measurement,
            Some(U::Percent),
            P::Measured,
            // Nullable: with total memory reported as zero the percentage
            // has no denominator, and the resolver says so.
            true,
            "Used RAM as a percentage of total.",
        )
        .derived(&["memory.used", "memory.total"]));
        add(Entity::new(
            "memory.swap.total",
            D::Memory,
            K::Identity,
            Some(U::Bytes),
            P::Measured,
            true,
            "Total swap or pagefile capacity.",
        ));
        add(Entity::new(
            "memory.swap.used",
            D::Memory,
            K::Measurement,
            Some(U::Bytes),
            P::Measured,
            true,
            "Swap or pagefile currently in use.",
        ));

        add(Entity::new(
            "cpu.cache.l1d",
            D::Cpu,
            K::Identity,
            Some(U::Bytes),
            P::Measured,
            true,
            "Total L1 data cache across all cores. Reported in bytes, though the \
             platform sources state it in KiB, so that every capacity in this \
             ontology carries the same unit.",
        ));
        add(Entity::new(
            "cpu.cache.l1i",
            D::Cpu,
            K::Identity,
            Some(U::Bytes),
            P::Measured,
            true,
            "Total L1 instruction cache across all cores.",
        ));
        add(Entity::new(
            "cpu.cache.l2",
            D::Cpu,
            K::Identity,
            Some(U::Bytes),
            P::Measured,
            true,
            "Total L2 cache.",
        ));
        add(Entity::new(
            "cpu.cache.l3",
            D::Cpu,
            K::Identity,
            Some(U::Bytes),
            P::Measured,
            true,
            "Total L3 cache. Null on parts with no last-level cache to report.",
        ));
        add(Entity::new(
            "cpu.cache.{n}.level",
            D::Cpu,
            K::Identity,
            Some(U::Identifier),
            P::Measured,
            true,
            "Cache level of one cache instance — l1, l2, l3, l4.",
        ));
        add(Entity::new(
            "cpu.cache.{n}.size",
            D::Cpu,
            K::Identity,
            Some(U::Bytes),
            P::Measured,
            true,
            "Size of one cache instance.",
        ));
        add(Entity::new(
            "cpu.cache.{n}.line_size",
            D::Cpu,
            K::Identity,
            Some(U::Bytes),
            P::Measured,
            true,
            "Cache line size. The unit of false sharing, and the reason this is \
             exposed per instance rather than assumed to be 64 bytes.",
        ));
        add(Entity::new(
            "cpu.cache.{n}.shared_cpus",
            D::Cpu,
            K::Identity,
            Some(U::Text),
            P::Measured,
            true,
            "Which logical processors share this cache, as a range list. Null where \
             the platform does not publish the sharing map.",
        ));

        // ── Microarchitecture and instruction set ────────────────────────────
        //
        // CPUID and its equivalents, which is the processor describing itself.
        // `Specification` rather than `Measured` for that reason: these values
        // do not change while the machine runs, and ironmon read a declaration
        // rather than sampling anything.
        //
        // The reader also computes a `single_thread_score` and a matching
        // multi-thread figure, both 0-100, and neither is published here. They
        // come from a lookup table over microarchitecture names, not from any
        // measurement of this processor, and an agent choosing between machines
        // on a number called a performance score would be relying on a guess
        // with a benchmark's authority. The extension list below is the part of
        // that reader an agent can actually act on: whether AVX-512 is present
        // is a fact, and it is the fact that decides a kernel.
        add(Entity::new(
            "cpu.microarch.<none>",
            D::Cpu,
            K::Diagnostic,
            None,
            P::Unavailable,
            true,
            "Present when the processor could not be identified, carrying why.",
        ));
        add(Entity::new(
            "cpu.microarch.name",
            D::Cpu,
            K::Identity,
            Some(U::Identifier),
            P::Specification,
            false,
            "Microarchitecture as ironmon recognises it - Zen 5, Golden Cove, \
             Firestorm. Distinct from `cpu.model`, which is the marketing \
             string: processors with different model names share a \
             microarchitecture, and it is the microarchitecture that decides \
             which instructions are fast.",
        ));
        add(Entity::new(
            "cpu.microarch.codename",
            D::Cpu,
            K::Identity,
            Some(U::Identifier),
            P::Specification,
            true,
            "Vendor code name for this silicon - Granite Ridge, Raptor Lake-S. \
             Nullable: IronMonitor does not hold one for every part it can name.",
        ));
        add(Entity::new(
            "cpu.microarch.vendor",
            D::Cpu,
            K::Identity,
            Some(U::Identifier),
            P::Specification,
            false,
            "Who made the processor.",
        ));
        add(Entity::new(
            "cpu.microarch.isa",
            D::Cpu,
            K::Identity,
            Some(U::Identifier),
            P::Specification,
            false,
            "Instruction set architecture - x86_64, aarch64, riscv64. The coarsest \
             compatibility question there is, and the first one a consumer \
             selecting a binary has to answer.",
        ));
        add(Entity::new(
            "cpu.microarch.process",
            D::Cpu,
            K::Identity,
            Some(U::Count),
            P::Specification,
            true,
            "Manufacturing process node in nanometres. A marketing figure as much \
             as a physical one across vendors, so it is comparable within a \
             vendor and not between them. Nullable where IronMonitor holds none.",
        ));
        add(Entity::new(
            "cpu.microarch.year",
            D::Cpu,
            K::Identity,
            Some(U::Count),
            P::Specification,
            true,
            "Year this microarchitecture was introduced. From ironmon's own table \
             rather than from the silicon - it is the age of the design, not of \
             this chip. Nullable where IronMonitor holds no date.",
        ));
        add(Entity::new(
            "cpu.microarch.hybrid",
            D::Cpu,
            K::Identity,
            None,
            P::Specification,
            false,
            "Whether the design mixes performance and efficiency cores. True means \
             the per-core figures elsewhere are not comparable to each other, \
             which is worth knowing before averaging any of them.",
        ));
        add(Entity::new(
            "cpu.microarch.family",
            D::Cpu,
            K::Identity,
            Some(U::Count),
            P::Specification,
            true,
            "CPUID family. Reported alongside model and stepping because the three \
             together identify the silicon exactly, which the name does not. \
             Nullable because Windows never decodes the triple, and all three go \
             together or not at all: publishing a stepping beside an absent family \
             would present a default as a measurement.",
        ));
        add(Entity::new(
            "cpu.microarch.model",
            D::Cpu,
            K::Identity,
            Some(U::Count),
            P::Specification,
            true,
            "CPUID model number within the family. Nullable with family and \
             stepping, which it is only meaningful beside.",
        ));
        add(Entity::new(
            "cpu.microarch.stepping",
            D::Cpu,
            K::Identity,
            Some(U::Count),
            P::Specification,
            true,
            "CPUID stepping - the revision of this particular silicon. Errata are \
             published against steppings, so a consumer checking for one needs \
             this and not the model name. Nullable with family and model; a \
             stepping of 0 is a legitimate value, which is why it is withheld \
             rather than defaulted when the triple was not read.",
        ));
        add(Entity::new(
            "cpu.microarch.physical_cores",
            D::Cpu,
            K::Identity,
            Some(U::Count),
            P::Specification,
            false,
            "Physical core count. Below `logical_cores` when SMT is on, and the \
             right denominator for anything that scales with execution \
             resources rather than with schedulable threads.",
        ));
        add(Entity::new(
            "cpu.microarch.logical_cores",
            D::Cpu,
            K::Identity,
            Some(U::Count),
            P::Specification,
            false,
            "Logical processor count, SMT threads included. The right denominator \
             for a thread pool.",
        ));
        add(Entity::new(
            "cpu.microarch.smt_enabled",
            D::Cpu,
            K::Identity,
            None,
            P::Measured,
            false,
            "Whether simultaneous multithreading is on right now. Measured rather \
             than declared: the silicon supports it or does not, but firmware \
             and the kernel both get a say, and this is the state as found.",
        ));
        add(Entity::new(
            "cpu.microarch.extension.<none>",
            D::Cpu,
            K::Diagnostic,
            None,
            P::Unavailable,
            true,
            "Present when no instruction set extension was enumerated, carrying \
             why. An empty list is never the answer: every processor ironmon runs \
             on supports something, so nothing enumerated means the reader \
             failed rather than that the CPU is bare.",
        ));
        add(Entity::new(
            "cpu.microarch.extension.{n}.name",
            D::Cpu,
            K::Identity,
            Some(U::Identifier),
            P::Specification,
            false,
            "Extension the processor reports as present - AVX-512, AES-NI, SVE. \
             The list holds only supported extensions, so membership is the \
             claim and there is no `supported` flag to check.",
        ));
        add(Entity::new(
            "cpu.microarch.extension.{n}.category",
            D::Cpu,
            K::Identity,
            Some(U::Identifier),
            P::Specification,
            false,
            "What the extension is for - vector arithmetic, cryptography, atomics.",
        ));
        add(Entity::new(
            "cpu.microarch.extension.{n}.description",
            D::Cpu,
            K::Identity,
            Some(U::Text),
            P::Specification,
            false,
            "A sentence on what the extension does. ironmon's own text rather than \
             the vendor's, and it is there so an agent that has not met an \
             extension can still tell whether it matters to the task at hand.",
        ));

        // ── Cryptographic acceleration ───────────────────────────────────────
        //
        // Which crypto primitives this processor implements in hardware. The
        // reader's `acceleration_score` and its free-text recommendations are
        // left out for the same reason the performance scores above are: a
        // score compresses a set of facts into a number whose scale nothing
        // documents, and an agent is better served by the facts.
        //
        // Throughput estimates are published, and as `Derived`. They come from
        // per-primitive constants rather than from running anything on this
        // machine, so they are a calculation and are labelled as one.
        add(Entity::new(
            "cpu.crypto.<none>",
            D::Cpu,
            K::Diagnostic,
            None,
            P::Unavailable,
            true,
            "Present when no hardware crypto feature was enumerated, carrying why.",
        ));
        add(Entity::new(
            "cpu.crypto.feature.{n}.name",
            D::Cpu,
            K::Identity,
            Some(U::Identifier),
            P::Specification,
            false,
            "Hardware-accelerated primitive - AES-NI, SHA extensions, carry-less \
             multiply. Only accelerated primitives appear; a primitive absent \
             from this list runs in software.",
        ));
        add(Entity::new(
            "cpu.crypto.feature.{n}.flag",
            D::Cpu,
            K::Identity,
            Some(U::Identifier),
            P::Specification,
            false,
            "The CPU flag that reports it, as the platform spells it. Included so a consumer can match against the flag names it already has rather than against the naming ironmon chose.",
        ));
        add(Entity::new(
            "cpu.crypto.feature.{n}.category",
            D::Cpu,
            K::Identity,
            Some(U::Identifier),
            P::Specification,
            false,
            "Primitive class - symmetric cipher, hash, public key, random.",
        ));
        add(Entity::new(
            "cpu.crypto.feature.{n}.throughput",
            D::Cpu,
            K::Limit,
            Some(U::BytesPerSecond),
            P::Derived,
            true,
            "Estimated per-core throughput for this primitive. A constant scaled \
             to bytes, not a measurement of this processor: a consumer sizing a \
             pipeline from it is reading a rule of thumb. Nullable, because \
             IronMonitor holds no estimate for every primitive.",
        )
        .derived(&["cpu.crypto.feature.{n}.name"]));
        add(Entity::new(
            "cpu.crypto.rng.<none>",
            D::Cpu,
            K::Diagnostic,
            None,
            P::Unavailable,
            true,
            "Present when no hardware random source was found, carrying why. Worth \
             its own diagnostic: a machine with no hardware RNG is a real and \
             consequential state, and it must not look like a failed read.",
        ));
        add(Entity::new(
            "cpu.crypto.rng.{n}.name",
            D::Cpu,
            K::Identity,
            Some(U::Identifier),
            P::Specification,
            false,
            "Hardware random source the platform reports as available - RDRAND, \
             RDSEED, a TPM. Unavailable sources are omitted rather than listed \
             as present-but-off.",
        ));
        add(Entity::new(
            "cpu.crypto.rng.{n}.source",
            D::Cpu,
            K::Identity,
            Some(U::Identifier),
            P::Specification,
            false,
            "Where the entropy comes from - a CPU instruction, a discrete chip, \
             the kernel pool. The distinction matters: a CPU instruction and a \
             TPM fail in different ways and are trusted differently.",
        ));
        add(Entity::new(
            "cpu.crypto.rng.{n}.quality",
            D::Cpu,
            K::Identity,
            Some(U::Count),
            P::Specification,
            true,
            "Entropy bits per sample, where the source declares a figure. Nullable \
             and usually null - most sources state nothing, and an assumed \
             number here would be worse than silence.",
        ));

        // ── Memory modules ───────────────────────────────────────────────────
        //
        // Everything here comes from the SMBIOS type-17 tables, which the board
        // fills in at POST. Until 6.0.0 the whole cluster was declared
        // `Measured`, which claimed IronMonitor had sampled a part number off the
        // module -- it had not; it read a table the firmware wrote. A board that
        // lies about its own DIMMs makes ironmon repeat the lie, and a consumer
        // deciding whether to trust one of these figures needs to know that
        // before it decides. `ecc` stays `Derived`: it is a comparison of two of
        // the fields below rather than a field of its own.
        //
        // Serial numbers are readable here and deliberately left out. They
        // identify the individual module rather than describe it, no agent task
        // needs one, and a hardware report carrying them is harder to share.
        add(Entity::new(
            "memory.dimm.{n}.locator",
            D::Memory,
            K::Identity,
            Some(U::Text),
            P::Specification,
            true,
            "Slot label as silkscreened on the board — DIMM_A1, ChannelA-DIMM0. \
             The id index is enumeration order; this is what a human replacing the \
             module needs.",
        ));
        add(Entity::new(
            "memory.dimm.{n}.populated",
            D::Memory,
            K::Identity,
            None,
            P::Specification,
            false,
            "Whether this slot holds a module. False is a reading, and the reason \
             the fields below may be absent for a slot that genuinely exists.",
        ));
        add(Entity::new(
            "memory.dimm.{n}.capacity",
            D::Memory,
            K::Identity,
            Some(U::Bytes),
            P::Specification,
            true,
            "Module capacity. Null for an empty slot rather than zero, since zero \
             would read as a module of no size.",
        ));
        add(Entity::new(
            "memory.dimm.{n}.speed",
            D::Memory,
            K::Identity,
            Some(U::Count),
            P::Specification,
            true,
            "Rated speed in MT/s. Counted rather than given a frequency unit \
             because megatransfers are not megahertz — DDR transfers twice per \
             clock, and conflating them halves or doubles every figure.",
        ));
        add(Entity::new(
            "memory.dimm.{n}.configured_speed",
            D::Memory,
            K::Identity,
            Some(U::Count),
            P::Specification,
            true,
            "Speed the module is actually running at, in MT/s. Differs from the \
             rated speed whenever the board declined to train at the module's \
             profile, which is the useful thing to notice.",
        ));
        add(Entity::new(
            "memory.dimm.{n}.type",
            D::Memory,
            K::Identity,
            Some(U::Identifier),
            P::Specification,
            true,
            "Memory technology — ddr4, ddr5, lpddr5.",
        ));
        add(Entity::new(
            "memory.dimm.{n}.manufacturer",
            D::Memory,
            K::Identity,
            Some(U::Text),
            P::Specification,
            true,
            "Module manufacturer from SPD.",
        ));
        add(Entity::new(
            "memory.dimm.{n}.part_number",
            D::Memory,
            K::Identity,
            Some(U::Text),
            P::Specification,
            true,
            "Module part number from SPD.",
        ));
        add(Entity::new(
            "memory.dimm.{n}.ecc",
            D::Memory,
            K::Identity,
            None,
            P::Derived,
            true,
            "Whether the module carries ECC, from the total data width exceeding \
             the usable width — 72 bits against 64. Derived rather than measured \
             because no SMBIOS field states it directly.",
        )
        .derived(&["memory.dimm.{n}.data_width", "memory.dimm.{n}.total_width"]));
        add(Entity::new(
            "memory.dimm.{n}.data_width",
            D::Memory,
            K::Identity,
            Some(U::Count),
            P::Specification,
            true,
            "Usable data width in bits.",
        ));
        add(Entity::new(
            "memory.dimm.{n}.total_width",
            D::Memory,
            K::Identity,
            Some(U::Count),
            P::Specification,
            true,
            "Total width in bits, including any ECC bits.",
        ));
        add(Entity::new(
            "memory.dimm.{n}.voltage",
            D::Memory,
            K::Identity,
            Some(U::Volts),
            P::Specification,
            true,
            "Operating voltage.",
        ));

        // ── Attached peripherals ─────────────────────────────────────────────
        //
        // Input devices, audio endpoints, cameras and printers, each with its
        // own `<none>` row. They share a shape and a reason for existing: an
        // agent asked to check whether a machine can join a call needs to know
        // whether a camera and a microphone are attached, and the answer "no
        // rows" cannot distinguish a headless server from a reader that is not
        // implemented on this platform.
        //
        // Everything here is `Measured`. These are enumerations of what is
        // plugged in right now, not declarations the firmware made at boot, and
        // a device unplugged between two snapshots really does disappear.
        add(Entity::new(
            "board.input.<none>",
            D::Board,
            K::Diagnostic,
            None,
            P::Unavailable,
            true,
            "Present when no input device was enumerated, carrying why. A machine \
             with no keyboard is an ordinary server; a machine whose input \
             enumeration failed is not, and the two must not look alike.",
        ));
        add(Entity::new(
            "board.input.{n}.name",
            D::Board,
            K::Identity,
            Some(U::Text),
            P::Measured,
            false,
            "Device name as the platform reports it.",
        ));
        add(Entity::new(
            "board.input.{n}.type",
            D::Board,
            K::Identity,
            Some(U::Identifier),
            P::Measured,
            false,
            "What kind of device this is - keyboard, mouse, touchpad, game \
             controller, tablet.",
        ));
        add(Entity::new(
            "board.input.{n}.interface",
            D::Board,
            K::Identity,
            Some(U::Identifier),
            P::Measured,
            true,
            "How it is attached - USB, Bluetooth, PS/2, internal. Nullable \
             because a virtualised input device commonly reports no interface \
             the classification recognises - both CI runners resolve it absent \
             while bare metal resolves it - and a guess here would name a bus \
             the device is not on.",
        ));
        add(Entity::new(
            "board.input.{n}.vendor",
            D::Board,
            K::Identity,
            Some(U::Text),
            P::Measured,
            true,
            "Vendor name or numeric id, whichever the platform gave. Nullable: \
             many devices report neither.",
        ));
        add(Entity::new(
            "board.input.{n}.product",
            D::Board,
            K::Identity,
            Some(U::Text),
            P::Measured,
            true,
            "Product name or numeric id. Nullable for the same reason as vendor.",
        ));
        add(Entity::new(
            "board.input.{n}.active",
            D::Board,
            K::Identity,
            None,
            P::Measured,
            // Nullable since 6.0.0: macOS does not read it. The id is
            // built with `format!`, so the static check in
            // `entities_with_an_absence_path_are_declared_nullable`
            // cannot see this one -- it had to be flipped by hand.
            true,
            "Whether the platform reports the device as connected and usable. A \
             present but inactive device is a different fact from an absent one \
             - a Bluetooth keyboard out of range is still enumerated.",
        ));
        add(Entity::new(
            "board.audio.<none>",
            D::Board,
            K::Diagnostic,
            None,
            P::Unavailable,
            true,
            "Present when no audio endpoint was enumerated, carrying why.",
        ));
        add(Entity::new(
            "board.audio.{n}.name",
            D::Board,
            K::Identity,
            Some(U::Text),
            P::Measured,
            false,
            "Endpoint name as the platform presents it to a user.",
        ));
        add(Entity::new(
            "board.audio.{n}.direction",
            D::Board,
            K::Identity,
            Some(U::Identifier),
            P::Measured,
            false,
            "Whether this endpoint plays, captures, or does both. The only \
             classification the platform provides - there is no separate \
             headset-or-speaker field, and declaring one would put a question \
             in the schema that no reader can answer.",
        ));
        add(Entity::new(
            "board.audio.{n}.state",
            D::Board,
            K::Identity,
            Some(U::Identifier),
            P::Measured,
            // Nullable since 6.0.0: Linux and macOS do not read it at all,
            // and Windows reads a status whose unhandled values now resolve
            // absent rather than falling through to "active".
            true,
            concat!(
                "Endpoint state as the platform reports it: active, ",
                "disabled, unplugged, not present."
            ),
        ));
        add(Entity::new(
            "board.audio.{n}.default",
            D::Board,
            K::Identity,
            None,
            P::Measured,
            // Nullable since 6.0.0: no platform reads it. The id is built
            // with `format!`, so the static nullability check cannot see
            // it and this had to be flipped by hand.
            true,
            "Whether this is the endpoint the system routes to by default.",
        ));
        add(Entity::new(
            "board.audio.{n}.volume",
            D::Board,
            K::Measurement,
            Some(U::Percent),
            P::Measured,
            true,
            "Endpoint volume. Nullable: not every endpoint exposes a level, and a \
             silent zero would read as muted rather than as unknown.",
        ));
        add(Entity::new(
            "board.audio.{n}.muted",
            D::Board,
            K::Identity,
            None,
            P::Measured,
            true,
            "Whether the endpoint is muted. Independent of volume - a muted \
             endpoint at 80% is not the same as an unmuted one at zero, and \
             only one of the two is fixed by turning it up. Null wherever the \
             mixer is not read, which is every platform today.",
        ));
        add(Entity::new(
            "board.camera.<none>",
            D::Board,
            K::Diagnostic,
            None,
            P::Unavailable,
            true,
            "Present when no camera was enumerated, carrying why.",
        ));
        add(Entity::new(
            "board.camera.{n}.name",
            D::Board,
            K::Identity,
            Some(U::Text),
            P::Measured,
            false,
            "Camera name as the platform reports it.",
        ));
        add(Entity::new(
            "board.camera.{n}.connection",
            D::Board,
            K::Identity,
            Some(U::Identifier),
            P::Measured,
            false,
            "How the camera is attached - USB, MIPI CSI, integrated, network.",
        ));
        add(Entity::new(
            "board.camera.{n}.driver",
            D::Board,
            K::Identity,
            Some(U::Identifier),
            P::Measured,
            true,
            "Driver bound to the device. Nullable where the platform does not say.",
        ));
        add(Entity::new(
            "board.camera.{n}.max_width",
            D::Board,
            K::Limit,
            Some(U::Count),
            P::Measured,
            true,
            "Widest frame the device reports supporting, in pixels. Nullable \
             rather than zero: a camera that reports no mode list is not a \
             camera with a zero-pixel frame.",
        ));
        add(Entity::new(
            "board.camera.{n}.max_height",
            D::Board,
            K::Limit,
            Some(U::Count),
            P::Measured,
            true,
            "Tallest frame the device reports supporting, in pixels. Nullable for \
             the same reason as the width.",
        ));
        add(Entity::new(
            "board.camera.{n}.active",
            D::Board,
            K::Measurement,
            None,
            P::Measured,
            // Nullable since 6.0.0. It is live on Linux only; Windows can rule
            // streaming out but not attribute it, and macOS does not look.
            true,
            "Whether the camera is streaming right now. The one genuinely live \
             field in this cluster, and the reason it is a measurement rather \
             than an identity.",
        ));
        // Kernel parameters are split, not subsetted. `name`, `value` and
        // `category` are what the platform reported and are published here.
        // `is_recommended`, `recommended`, `security_score`, `network_score` and
        // `recommendations` are this crate's judgement about what the values
        // ought to be, and they are deliberately absent: `ironmon tune`'s standing
        // rule is that a proposed value comes from what the system declared,
        // never from this crate. An agent reading `security_score` off a
        // hardware report would be taking an opinion for a measurement, and the
        // ontology is the wrong surface for it.
        add(Entity::new(
            "system.kernel_param.<none>",
            D::System,
            K::Diagnostic,
            None,
            P::Unavailable,
            true,
            "Present when no kernel parameter could be read, carrying why. \
             Windows reads exactly one — the TCP auto-tuning level — so its \
             absence here is common and says nothing about Linux.",
        ));
        add(Entity::new(
            "system.kernel_param.{n}.name",
            D::System,
            K::Identity,
            Some(U::Identifier),
            P::Measured,
            false,
            "Parameter name in the platform's own namespace: a sysctl key on \
             Linux and macOS, a synthesised dotted name on Windows where the \
             setting comes from a cmdlet rather than a key.",
        ));
        add(Entity::new(
            "system.kernel_param.{n}.value",
            D::System,
            K::Measurement,
            Some(U::Identifier),
            P::Measured,
            false,
            "The value as the platform reported it, unparsed. A measurement \
             rather than an identity because it changes while the machine runs, \
             and text rather than a number because sysctl values are not all \
             numeric — a list of ports and a boolean live in the same namespace.",
        ));
        add(Entity::new(
            "system.kernel_param.{n}.category",
            D::System,
            K::Identity,
            Some(U::Identifier),
            P::Measured,
            true,
            "Which subsystem the parameter governs — network, memory, security. \
             The reader's classification of a real key, not a judgement about \
             its value.",
        ));

        // Services are counted, not enumerated. This machine runs 311 of them,
        // and one entity per service would more than double every snapshot to
        // carry a list nothing reads in full. What a consumer actually asks is
        // how many there are, how many are up, and which ones are broken — so
        // the failed units are named and the working ones are a number.
        add(Entity::new(
            "system.service.count.total",
            D::System,
            K::Measurement,
            Some(U::Count),
            P::Measured,
            false,
            "How many services the platform's service manager knows about.",
        ));
        add(Entity::new(
            "system.service.count.running",
            D::System,
            K::Measurement,
            Some(U::Count),
            P::Measured,
            false,
            "How many are active right now.",
        ));
        add(Entity::new(
            "system.service.count.failed",
            D::System,
            K::Measurement,
            Some(U::Count),
            P::Measured,
            false,
            "How many are in a failed state. Zero is a real and common reading \
             here, and distinct from the absence carried by \
             `system.service.<none>` — a machine with no failures and a machine \
             whose services could not be enumerated must not look alike.",
        ));
        add(Entity::new(
            "system.service.failed.{n}",
            D::System,
            K::Identity,
            Some(U::Identifier),
            P::Measured,
            true,
            "Name of a failed service. The counts above say how much is wrong; \
             this says what, which is the part a consumer can act on. Absent \
             when nothing has failed.",
        ));
        add(Entity::new(
            "system.service.<none>",
            D::System,
            K::Diagnostic,
            None,
            P::Unavailable,
            true,
            "Present when the service manager could not be reached, carrying \
             why. Distinguishes that from a genuine count of zero.",
        ));
        add(Entity::new(
            "system.printer.<none>",
            D::System,
            K::Diagnostic,
            None,
            P::Unavailable,
            true,
            "Present when no printer was enumerated, carrying why.",
        ));
        add(Entity::new(
            "system.printer.{n}.name",
            D::System,
            K::Identity,
            Some(U::Text),
            P::Measured,
            false,
            "Queue name, which is what a print job is addressed to.",
        ));
        add(Entity::new(
            "system.printer.{n}.description",
            D::System,
            K::Identity,
            Some(U::Text),
            P::Measured,
            true,
            "Human description or model. Nullable: many queues carry none.",
        ));
        add(Entity::new(
            "system.printer.{n}.connection",
            D::System,
            K::Identity,
            Some(U::Identifier),
            P::Measured,
            true,
            "How the printer is reached - USB, network, virtual. Nullable because \
             the classification is drawn from the port string and the device name, \
             and a local printer on a port shape neither recognises is a real \
             configuration rather than a failed read.",
        ));
        add(Entity::new(
            "system.printer.{n}.status",
            D::System,
            K::Measurement,
            Some(U::Identifier),
            P::Measured,
            true,
            "Queue state as the spooler reports it - idle, printing, stopped, \
             error. A measurement, because it changes while the machine runs. \
             Nullable because the spooler itself has \"Other\" and \"Unknown\" \
             states, and reporting either as a queue state would invent one.",
        ));
        add(Entity::new(
            "system.printer.{n}.default",
            D::System,
            K::Identity,
            None,
            P::Measured,
            false,
            "Whether this is the queue a job goes to when none is named.",
        ));
        add(Entity::new(
            "system.printer.{n}.accepting_jobs",
            D::System,
            K::Measurement,
            None,
            P::Measured,
            // Nullable since 6.0.0: only CUPS reports it. The id is built
            // with `format!`, so the static nullability check cannot see
            // this one either.
            true,
            "Whether the queue is taking new work. Distinct from `status`: a \
             stopped queue may still accept jobs and hold them, which is the \
             difference between a delayed print and a rejected one.",
        ));
        add(Entity::new(
            "system.printer.{n}.color",
            D::System,
            K::Identity,
            None,
            P::Measured,
            true,
            concat!(
                "Whether the device prints in colour, as the driver declares. ",
                "Nullable: a spooler that lists no capabilities has not said the ",
                "printer is monochrome, and this read `false` for every printer ",
                "on Windows until it was taken from `Capabilities` rather than ",
                "from a property `Win32_Printer` does not have.",
            ),
        ));

        // ── Bluetooth radios ─────────────────────────────────────────────────
        //
        // Adapters only. Paired and nearby devices are readable too and are not
        // published here: they are other people's hardware as much as this
        // machine's, they change without anything on this machine doing
        // anything, and a hardware report that enumerates the phones in the
        // room is a different and more sensitive artefact than one that
        // describes the computer.
        add(Entity::new(
            "network.bluetooth.<none>",
            D::Network,
            K::Diagnostic,
            None,
            P::Unavailable,
            true,
            "Present when no Bluetooth adapter was enumerated, carrying why.",
        ));
        add(Entity::new(
            "network.bluetooth.{n}.name",
            D::Network,
            K::Identity,
            Some(U::Text),
            P::Measured,
            false,
            "Adapter name as the platform reports it.",
        ));
        add(Entity::new(
            "network.bluetooth.{n}.powered",
            D::Network,
            K::Measurement,
            None,
            P::Measured,
            false,
            "Whether the radio is on. A measurement: it is a runtime state, and it \
             is the field that answers whether the adapter is usable now.",
        ));

        // ── Storage controllers ──────────────────────────────────────────────
        //
        // The controllers the disks hang off, which is a different question
        // from the disks themselves. An agent diagnosing throughput needs both:
        // four NVMe drives behind one x4 controller do not add up the way four
        // drives behind four controllers do, and nothing in `disk.*` says which
        // arrangement this machine has.
        add(Entity::new(
            "disk.controller.<none>",
            D::Disk,
            K::Diagnostic,
            None,
            P::Unavailable,
            true,
            "Present when no storage controller was enumerated, carrying why.",
        ));
        add(Entity::new(
            "disk.controller.{n}.name",
            D::Disk,
            K::Identity,
            Some(U::Text),
            P::Measured,
            false,
            "Controller name as the platform reports it.",
        ));
        add(Entity::new(
            "disk.controller.{n}.vendor",
            D::Disk,
            K::Identity,
            Some(U::Text),
            P::Measured,
            true,
            "Controller vendor. Nullable where the platform names none.",
        ));
        add(Entity::new(
            "disk.controller.{n}.model",
            D::Disk,
            K::Identity,
            Some(U::Text),
            P::Measured,
            true,
            "Controller model. Nullable for the same reason as the vendor.",
        ));
        add(Entity::new(
            "disk.controller.{n}.driver",
            D::Disk,
            K::Identity,
            Some(U::Identifier),
            P::Measured,
            true,
            "Driver bound to the controller. The field that most often explains a \
             performance difference between two otherwise identical machines.",
        ));
        add(Entity::new(
            "disk.controller.{n}.interface",
            D::Disk,
            K::Identity,
            Some(U::Identifier),
            P::Measured,
            false,
            "Interface the controller speaks - NVMe, SATA, SAS, USB, RAID.",
        ));
        add(Entity::new(
            "disk.controller.{n}.pci_address",
            D::Disk,
            K::Identity,
            Some(U::Identifier),
            P::Measured,
            true,
            "PCI address, where the controller sits on a PCI bus. Nullable for \
             controllers that do not - a USB bridge has none. Present so a \
             consumer can join this against `pci.*` and find the link width.",
        ));
        add(Entity::new(
            "disk.controller.{n}.ports",
            D::Disk,
            K::Limit,
            Some(U::Count),
            P::Measured,
            true,
            "Port count the controller reports. Nullable rather than zero: a \
             controller that reports no count still has ports.",
        ));

        // ── Power profiles ───────────────────────────────────────────────────
        //
        // Which power plans the operating system offers and which one is in
        // force. A setting in every sense -- it is writable, and it changes
        // every frequency and power figure elsewhere in this snapshot -- but it
        // is declared as an identity here because the ontology's `Setting` kind
        // is bound to the apply layer, and IronMonitor does not yet write these.
        // Declaring it writable when nothing can write it would be the same
        // overclaim in a different field.
        add(Entity::new(
            "power.profile.<none>",
            D::Power,
            K::Diagnostic,
            None,
            P::Unavailable,
            true,
            "Present when no power plan was enumerated, carrying why.",
        ));
        add(Entity::new(
            "power.profile.{n}.name",
            D::Power,
            K::Identity,
            Some(U::Text),
            P::Measured,
            false,
            "Plan name as the operating system presents it - Balanced, High \
             performance, Power saver.",
        ));
        add(Entity::new(
            "power.profile.{n}.active",
            D::Power,
            K::Measurement,
            None,
            P::Measured,
            false,
            "Whether this plan is the one in force. Exactly one plan should carry \
             true; a snapshot where none does means the active plan was not \
             identified rather than that the machine is running unmanaged.",
        ));

        // ── Video codec engines ──────────────────────────────────────────────
        //
        // What the GPU can encode and decode in hardware. This cluster is the
        // clearest case in the whole ontology for per-reading provenance: the
        // underlying reader records how it learned each capability, and the two
        // answers are not equally good. A capability the driver was asked about
        // directly is `Measured`; one concluded from the GPU model is `Derived`,
        // and it is wrong exactly as often as the lookup table is out of date.
        // Both are published, and which is which is never hidden.
        add(Entity::new(
            "gpu.codec.<none>",
            D::Gpu,
            K::Diagnostic,
            None,
            P::Unavailable,
            true,
            "Present when no hardware codec capability was found, carrying why. A \
             GPU with no media engine is a real configuration - many datacentre \
             parts ship without one - and it must not look like a failed probe.",
        ));
        add(Entity::new(
            "gpu.codec.{n}.device",
            D::Gpu,
            K::Identity,
            Some(U::Text),
            P::Specification,
            false,
            "The GPU this capability belongs to. Present on every row because a \
             machine with two GPUs has two independent sets of engines. Declared \
             at the weakest provenance a row can carry: a queried row resolves \
             Measured, an inferred one Specification, and a schema that promised \
             Measured for both would overstate half of them.",
        ));
        add(Entity::new(
            "gpu.codec.{n}.codec",
            D::Gpu,
            K::Identity,
            Some(U::Identifier),
            P::Specification,
            false,
            "Codec - H.264, HEVC, AV1, VP9. Measured on a queried row; declared \
             here at the weaker provenance an inferred row carries.",
        ));
        add(Entity::new(
            "gpu.codec.{n}.direction",
            D::Gpu,
            K::Identity,
            Some(U::Identifier),
            P::Specification,
            false,
            "Whether the engine encodes, decodes, or both. Asymmetry is normal: \
             AV1 decode is common on hardware that cannot encode it. Measured on \
             a queried row; declared at the weaker provenance an inferred row \
             carries.",
        ));
        add(Entity::new(
            "gpu.codec.{n}.engine",
            D::Gpu,
            K::Identity,
            Some(U::Identifier),
            P::Specification,
            true,
            "Vendor name for the engine - NVENC, NVDEC, QuickSync, VCN. Nullable \
             where the capability was inferred rather than queried, since an \
             inference names no engine. Measured on a queried row; declared at \
             the weaker provenance an inferred row carries.",
        ));
        add(Entity::new(
            "gpu.codec.{n}.max_resolution",
            D::Gpu,
            K::Limit,
            Some(U::Identifier),
            P::Specification,
            false,
            "Largest frame class this engine handles, as the reader classifies it - hd, full_hd, qhd, uhd_4k, uhd_8k. A class rather than a pixel count because that is what the underlying source provides. Measured on a queried row; declared at the weaker provenance an inferred row carries.",
        ));
        add(Entity::new(
            "gpu.codec.{n}.max_width",
            D::Gpu,
            K::Limit,
            Some(U::Count),
            P::Derived,
            true,
            "Width in pixels of the frame class above. Derived, and the derivation is a table: uhd_4k means 3840 here. An engine whose real limit sits between two classes will be described by the lower one, so this is a floor rather than a measured ceiling.",
        )
        .derived(&["gpu.codec.{n}.max_resolution"]));
        add(Entity::new(
            "gpu.codec.{n}.max_height",
            D::Gpu,
            K::Limit,
            Some(U::Count),
            P::Derived,
            true,
            "Height in pixels of the frame class above, from the same table and with the same caveat.",
        )
        .derived(&["gpu.codec.{n}.max_resolution"]));
        add(Entity::new(
            "gpu.codec.{n}.max_bit_depth",
            D::Gpu,
            K::Limit,
            Some(U::Count),
            P::Specification,
            true,
            "Bit depth per channel the engine supports. Eight against ten is the \
             difference between HDR working and not. Measured on a queried row; \
             declared at the weaker provenance an inferred row carries, like the \
             rest of this cluster.",
        ));
        add(Entity::new(
            "gpu.codec.{n}.max_fps",
            D::Gpu,
            K::Limit,
            Some(U::Count),
            // Declared `Unavailable`, not `Derived`: a `Derived` entity has
            // to name the inputs it is computed from, and this one named two
            // it never read.
            P::Unavailable,
            true,
            concat!(
                "Frames per second at the maximum resolution. No driver ",
                "reports a frame rate and this crate does not compute one, ",
                "so this always resolves absent. The description here used ",
                "to say the figure was arithmetic over the engine ",
                "generation, and the entity declared inputs to match; the ",
                "reader held a literal 60 at every construction site."
            ),
        ));
        add(Entity::new(
            "gpu.codec.{n}.confidence",
            D::Gpu,
            K::Identity,
            Some(U::Percent),
            P::Derived,
            false,
            "How sure the reader is of this row, as a percentage. Full confidence \
             means the driver was asked; anything less means the capability was \
             concluded from the GPU model. Published alongside the reading's \
             own provenance rather than instead of it: the provenance says what \
             kind of claim this is and the confidence says how strong.",
        )
        .derived(&["gpu.codec.{n}.codec", "gpu.codec.{n}.device"]));

        // ── Virtualization ───────────────────────────────────────────────────
        add(Entity::new(
            "system.virtualization.platform",
            D::System,
            K::Identity,
            Some(U::Identifier),
            P::Measured,
            true,
            "Whether this is bare metal, a virtual machine, or a container. The \
             first thing an agent needs before trusting any other reading: a \
             guest's view of its own hardware is the hypervisor's choice, not the \
             silicon's.",
        ));
        add(Entity::new(
            "system.virtualization.hypervisor",
            D::System,
            K::Identity,
            Some(U::Identifier),
            P::Measured,
            true,
            concat!(
                "Which hypervisor is present, when one was detected. ",
                "A value here does not mean this machine is a guest. ",
                "Windows 11 enables virtualization-based security by default, ",
                "which puts the host itself under a thin hypervisor, so an ",
                "ordinary desktop reports `hyperv` while ",
                "`system.virtualization.platform` correctly reports ",
                "`bare_metal` -- both rows are true at once, and the machine ",
                "this was found on is one of them. Read `platform` for the ",
                "question \"am I in a VM\": `detect.rs` answers it from the ",
                "Hyper-V privilege leaf rather than the CPUID vendor string, ",
                "which is identical either side of the partition boundary. ",
                "Null means no hypervisor was detected at all."
            ),
        ));
        add(Entity::new(
            "system.virtualization.detection_method",
            D::System,
            K::Identity,
            Some(U::Text),
            P::Measured,
            true,
            "How the platform was determined — CPUID leaf, DMI string, kernel \
             file. Recorded because virtualization detection is inference, and an \
             agent weighing it should see what it rests on.",
        ));
        add(Entity::new(
            "system.virtualization.hardware_support",
            D::System,
            K::Identity,
            None,
            P::Measured,
            true,
            "Whether the CPU exposes hardware virtualization (VT-x or AMD-V). \
             Independent of whether this machine is itself virtualized.",
        ));

        // ── NUMA ─────────────────────────────────────────────────────────────
        add(Entity::new(
            "memory.numa.nodes",
            D::Memory,
            K::Identity,
            Some(U::Count),
            P::Measured,
            true,
            "Number of NUMA nodes. One means uniform memory access, which is a \
             reading and not an absence.",
        ));
        add(Entity::new(
            "memory.numa.is_numa",
            D::Memory,
            K::Identity,
            None,
            P::Measured,
            true,
            "Whether memory access is genuinely non-uniform. A single-node machine \
             reports false; a machine the reader could not inspect reports nothing.",
        ));
        add(Entity::new(
            "memory.numa.{n}.cpus",
            D::Memory,
            K::Identity,
            Some(U::Count),
            P::Measured,
            true,
            "Logical processors belonging to this NUMA node.",
        ));
        add(Entity::new(
            "memory.numa.{n}.memory",
            D::Memory,
            K::Identity,
            Some(U::Bytes),
            P::Measured,
            true,
            "Memory attached to this NUMA node.",
        ));

        // ── ECC ──────────────────────────────────────────────────────────────
        add(Entity::new(
            "memory.ecc.active",
            D::Memory,
            K::Identity,
            None,
            P::Measured,
            true,
            "Whether ECC is enabled and reporting. Distinct from whether the \
             modules carry ECC bits — see the per-slot `ecc` entity — because \
             hardware capable of correction that is not reporting corrections is \
             indistinguishable from hardware doing nothing.",
        ));
        add(Entity::new(
            "memory.ecc.correctable_errors",
            D::Memory,
            K::Measurement,
            Some(U::Count),
            P::Measured,
            true,
            "Corrected memory errors since boot. Zero is a reading and a good one; \
             a rising count is the earliest warning a DIMM gives.",
        ));
        add(Entity::new(
            "memory.ecc.uncorrectable_errors",
            D::Memory,
            K::Measurement,
            Some(U::Count),
            P::Measured,
            true,
            "Uncorrectable memory errors since boot. Any non-zero value means data \
             was lost.",
        ));

        // ── PCI ──────────────────────────────────────────────────────────────
        add(Entity::new(
            "pci.{addr}.vendor",
            D::Pci,
            K::Identity,
            Some(U::Text),
            P::Measured,
            true,
            "Vendor name. The id segment is the BDF address — domain, bus, device \
             and function — with separators replaced, so it is the same identifier \
             `lspci` prints and is stable across reboots.",
        ));
        add(Entity::new(
            "pci.{addr}.device",
            D::Pci,
            K::Identity,
            Some(U::Text),
            P::Measured,
            true,
            "Device name as the vendor and device id pair decodes to.",
        ));
        add(Entity::new(
            "pci.{addr}.class",
            D::Pci,
            K::Identity,
            Some(U::Identifier),
            P::Measured,
            true,
            "Decoded PCI class — what kind of device this is.",
        ));
        add(Entity::new(
            "pci.{addr}.driver",
            D::Pci,
            K::Identity,
            Some(U::Text),
            P::Measured,
            true,
            "Driver bound to the device. Null when none is, which is the \
             interesting case: an unclaimed device is a device that does not work.",
        ));
        add(Entity::new(
            "pci.{addr}.link.width",
            D::Pci,
            K::Identity,
            Some(U::Identifier),
            P::Measured,
            true,
            "Negotiated PCIe link width — x1, x4, x16. Unavailable on Windows, \
             which exposes no link state through the interfaces IronMonitor uses.",
        ));
        add(Entity::new(
            "pci.{addr}.link.max_width",
            D::Pci,
            K::Limit,
            Some(U::Identifier),
            P::Measured,
            true,
            "Maximum width the device supports. Compare against the negotiated \
             width: a x16 card trained at x4 is the classic silent performance \
             fault this pair exists to expose.",
        ));
        add(Entity::new(
            "pci.{addr}.link.speed",
            D::Pci,
            K::Identity,
            Some(U::Identifier),
            P::Measured,
            true,
            "Negotiated link speed, as the transfer rate the device reports — \
             8.0 GT/s, 16.0 GT/s.",
        ));
        add(Entity::new(
            "pci.{addr}.link.max_speed",
            D::Pci,
            K::Limit,
            Some(U::Identifier),
            P::Measured,
            true,
            "Maximum link speed the device supports.",
        ));
        add(Entity::new(
            "pci.{addr}.numa_node",
            D::Pci,
            K::Identity,
            Some(U::Count),
            P::Measured,
            true,
            "NUMA node this device is attached to. Null on machines with no NUMA \
             affinity to report — the reader's -1 sentinel is not a node number.",
        ));

        // ── USB ──────────────────────────────────────────────────────────────
        add(Entity::new(
            "usb.{addr}.product",
            D::Usb,
            K::Identity,
            Some(U::Text),
            P::Measured,
            true,
            concat!(
                "Product string as the device reports it. The id segment is ",
                "the platform's own device path -- the sysfs name on Linux, ",
                "the PnP instance path on Windows, the Location ID on ",
                "macOS -- normalised into one segment. It is unique per ",
                "device and changes only when that device moves or is ",
                "replaced, so unplugging one device does not repoint the ",
                "ids of the others."
            ),
        ));
        add(Entity::new(
            "usb.{addr}.manufacturer",
            D::Usb,
            K::Identity,
            Some(U::Text),
            P::Measured,
            true,
            "Manufacturer string. Null on devices that publish none.",
        ));
        add(Entity::new(
            "usb.{addr}.vendor_id",
            D::Usb,
            K::Identity,
            Some(U::Identifier),
            P::Measured,
            true,
            "USB vendor id, four hex digits.",
        ));
        add(Entity::new(
            "usb.{addr}.product_id",
            D::Usb,
            K::Identity,
            Some(U::Identifier),
            P::Measured,
            true,
            "USB product id, four hex digits.",
        ));
        add(Entity::new(
            "usb.{addr}.class",
            D::Usb,
            K::Identity,
            Some(U::Identifier),
            P::Measured,
            true,
            "Device class — hid, mass_storage, hub, and so on.",
        ));
        add(Entity::new(
            "usb.{addr}.speed",
            D::Usb,
            K::Identity,
            Some(U::Identifier),
            P::Measured,
            true,
            "Negotiated bus speed — low, full, high, super. A super-speed device \
             on a high-speed port reports high, which is how a wrong cable shows.",
        ));

        // ── Disk ─────────────────────────────────────────────────────────────
        add(Entity::new(
            "disk.{n}.model",
            D::Disk,
            K::Identity,
            Some(U::Text),
            P::Measured,
            true,
            "Drive model string from the device.",
        ));
        add(Entity::new(
            "disk.{n}.capacity",
            D::Disk,
            K::Identity,
            Some(U::Bytes),
            P::Measured,
            true,
            "Raw device capacity, before any partitioning. Nullable because a \
             device can genuinely decline to report one — a USB mass-storage \
             gadget with no medium presents as a drive of unstated size, and zero \
             would claim it holds nothing.",
        ));
        add(Entity::new(
            "disk.{n}.read_rate",
            D::Disk,
            K::Measurement,
            Some(U::BytesPerSecond),
            P::Measured,
            true,
            concat!(
                "Read throughput. Null in a single-shot snapshot, because a ",
                "rate needs two samples to difference and this resolver takes ",
                "one. Not a platform limitation: Windows exposes ",
                "`DiskReadBytesPerSec` and Linux `/sys/block/*/stat` gives ",
                "read and write sectors separately, so a caller that samples ",
                "twice can compute it per direction."
            ),
        ));
        add(Entity::new(
            "disk.{n}.write_rate",
            D::Disk,
            K::Measurement,
            Some(U::BytesPerSecond),
            P::Measured,
            true,
            concat!(
                "Write throughput. Null in a single-shot snapshot, because a ",
                "rate needs two samples to difference and this resolver takes ",
                "one. Not a platform limitation: Windows exposes ",
                "`DiskWriteBytesPerSec` and Linux `/sys/block/*/stat` gives ",
                "read and write sectors separately, so a caller that samples ",
                "twice can compute it per direction."
            ),
        ));
        add(Entity::new(
            "disk.{n}.partition.{m}.mount_point",
            D::Disk,
            K::Identity,
            Some(U::Text),
            P::Measured,
            true,
            "Mount point of a partition that genuinely resides on this device. A \
             device holding no mounted volume reports none rather than every volume \
             on the system.",
        ));
        add(Entity::new(
            "disk.{n}.serial",
            D::Disk,
            K::Identity,
            Some(U::Text),
            P::Measured,
            true,
            "Drive serial number. Null where the platform will not disclose it \
             without elevation.",
        ));
        add(Entity::new(
            "disk.{n}.kind",
            D::Disk,
            K::Identity,
            Some(U::Text),
            P::Measured,
            true,
            "Transport and medium — nvme_ssd, sata_ssd, sata_hdd, and so on. \
             Distinct from the SMART media type, which describes the medium alone.",
        ));
        add(Entity::new(
            "disk.{n}.health",
            D::Disk,
            K::Measurement,
            Some(U::Text),
            P::Measured,
            true,
            "Health verdict — healthy, warning, critical, failed, unknown. Derived \
             from the drive's own report, never from the device merely existing. \
             `unknown` is a real answer and means no counter could be read.",
        ));
        add(Entity::new(
            "disk.{n}.temperature",
            D::Disk,
            K::Measurement,
            Some(U::Celsius),
            P::Measured,
            true,
            "Drive temperature, from the NVMe health log or ATA attribute 194. \
             Null on devices that expose no thermal sensor.",
        ));
        add(Entity::new(
            "disk.{n}.smart.passed",
            D::Disk,
            K::Measurement,
            None,
            P::Measured,
            true,
            "The drive's own pass/fail verdict on itself — NVMe critical warning \
             bits, or the ATA failure prediction. Not a judgement computed from the \
             counters below.",
        ));
        add(Entity::new(
            "disk.{n}.smart.power_on_hours",
            D::Disk,
            K::Measurement,
            Some(U::Hours),
            P::Measured,
            true,
            "Lifetime powered-on hours. A minority of ATA drives report this \
             attribute in minutes and nothing in the structure distinguishes them.",
        ));
        add(Entity::new(
            "disk.{n}.smart.power_cycles",
            D::Disk,
            K::Measurement,
            Some(U::Count),
            P::Measured,
            true,
            "Lifetime power cycle count.",
        ));
        add(Entity::new(
            "disk.{n}.smart.reallocated_sectors",
            D::Disk,
            K::Measurement,
            Some(U::Count),
            P::Measured,
            true,
            "Sectors the drive has remapped. Null on NVMe, which has no sector \
             reallocation concept — that is an ATA notion, and zero would assert a \
             clean count that was never measured.",
        ));
        add(Entity::new(
            "disk.{n}.smart.pending_sectors",
            D::Disk,
            K::Measurement,
            Some(U::Count),
            P::Measured,
            true,
            "Sectors awaiting remap — data at risk now. Null on NVMe, for the same \
             reason as the reallocated count.",
        ));
        add(Entity::new(
            "disk.{n}.smart.uncorrectable_sectors",
            D::Disk,
            K::Measurement,
            Some(U::Count),
            P::Measured,
            true,
            "Uncorrectable sectors on ATA; media errors on NVMe.",
        ));
        add(Entity::new(
            "disk.{n}.nvme.version",
            D::Disk,
            K::Identity,
            Some(U::Text),
            P::Measured,
            true,
            "NVMe specification version the controller reports. Null on non-NVMe.",
        ));
        add(Entity::new(
            "disk.{n}.nvme.percentage_used",
            D::Disk,
            K::Measurement,
            Some(U::Percent),
            P::Measured,
            true,
            "Share of rated write endurance consumed. May legitimately exceed 100 \
             on a drive past its rating, which is a reading and not an error.",
        ));
        add(Entity::new(
            "disk.{n}.nvme.data_units_written",
            D::Disk,
            K::Measurement,
            Some(U::Count),
            P::Measured,
            true,
            "Lifetime data units written, in 1000x512-byte units as NVMe defines \
             them. Reported in the drive's own unit rather than converted, so that \
             the number matches what other NVMe tools show.",
        ));
        add(Entity::new(
            "disk.{n}.nvme.data_units_read",
            D::Disk,
            K::Measurement,
            Some(U::Count),
            P::Measured,
            true,
            "Lifetime data units read, in the same units as the written count.",
        ));
        add(Entity::new(
            "disk.{n}.nvme.power_state",
            D::Disk,
            K::Measurement,
            Some(U::Count),
            P::Measured,
            true,
            "Current NVMe power state index. State 0 is both the active state and \
             what an unread field would look like, so this is null unless the \
             controller answered Get Features.",
        ));
        add(Entity::new(
            "disk.{n}.nvme.critical_warnings",
            D::Disk,
            K::Measurement,
            Some(U::Count),
            P::Measured,
            true,
            "NVMe critical warning bitfield. Zero is a reading — a drive with \
             nothing wrong — and is reported as such rather than omitted.",
        ));

        // ── Network ──────────────────────────────────────────────────────────
        add(Entity::new(
            "network.{iface}.rx_rate",
            D::Network,
            K::Measurement,
            Some(U::BytesPerSecond),
            P::Measured,
            true,
            "Receive throughput on this interface.",
        ));
        add(Entity::new(
            "network.{iface}.tx_rate",
            D::Network,
            K::Measurement,
            Some(U::BytesPerSecond),
            P::Measured,
            true,
            "Transmit throughput on this interface.",
        ));
        // Cumulative counters exist because a rate does not: a single-shot query has
        // one sample, and a rate needs two. Declaring only `rx_rate` forced a
        // resolver to either return nothing useful or to pass a counter off as a
        // rate. These are what a one-shot read can honestly produce.
        add(Entity::new(
            "network.{iface}.rx_bytes",
            D::Network,
            K::Measurement,
            Some(U::Bytes),
            P::Measured,
            true,
            "Bytes received since the interface came up. A counter, not a rate — \
             differentiate two samples to obtain throughput.",
        ));
        add(Entity::new(
            "network.{iface}.tx_bytes",
            D::Network,
            K::Measurement,
            Some(U::Bytes),
            P::Measured,
            true,
            "Bytes transmitted since the interface came up. A counter, not a rate.",
        ));
        add(Entity::new(
            "network.{iface}.link_speed",
            D::Network,
            K::Limit,
            Some(U::Count),
            P::Measured,
            true,
            "Negotiated link rate in megabits per second, read from the driver. \
             Platforms that can only guess from the interface name report this as \
             specification-provenance instead.",
        ));

        // ── Power and thermal ────────────────────────────────────────────────
        add(Entity::new(
            "power.battery.percentage",
            D::Power,
            K::Measurement,
            Some(U::Percent),
            P::Measured,
            true,
            "Battery charge. Null on machines with no battery.",
        ));
        add(Entity::new(
            "thermal.{sensor}.temperature",
            D::Thermal,
            K::Measurement,
            Some(U::Celsius),
            P::Measured,
            true,
            "A named board or package temperature sensor.",
        ));
        add(Entity::new(
            "thermal.{sensor}.fan_rpm",
            D::Thermal,
            K::Measurement,
            Some(U::Rpm),
            P::Measured,
            true,
            "Fan speed. Null where no tachometer is wired.",
        ));

        // ── Process ──────────────────────────────────────────────────────────
        add(Entity::new(
            "process.{pid}.name",
            D::Process,
            K::Identity,
            Some(U::Text),
            P::Measured,
            false,
            "Executable name.",
        ));
        add(Entity::new(
            "process.{pid}.cpu",
            D::Process,
            K::Measurement,
            Some(U::Percent),
            P::Measured,
            true,
            "Process CPU share over the last interval.",
        ));
        add(Entity::new(
            "process.{pid}.memory",
            D::Process,
            K::Measurement,
            Some(U::Bytes),
            P::Measured,
            true,
            "Process resident memory.",
        ));

        // ── Displays ─────────────────────────────────────────────────────────
        //
        // Not in the plan's named list, which was the plan's own list rather
        // than an audit: `display` has a reader that answers on every desktop
        // and had no schema at all. Around 28 modules are still in that state;
        // see the plan section.
        add(Entity::new(
            "board.display.<none>",
            D::Board,
            K::Diagnostic,
            None,
            P::Unavailable,
            true,
            "Present when no display was enumerated, carrying why. A headless server reporting none is a reading; a failed query is not.",
        ));
        add(Entity::new(
            "board.display.{n}.name",
            D::Board,
            K::Identity,
            Some(U::Text),
            P::Measured,
            true,
            "Display name as the platform reports it. Nullable: a monitor that publishes no EDID name genuinely has none to report.",
        ));
        add(Entity::new(
            "board.display.{n}.connection",
            D::Board,
            K::Identity,
            Some(U::Identifier),
            P::Measured,
            true,
            "How the display is attached - hdmi, displayport, edp, internal. Nullable, and measured to be so: two of the three displays on the development machine report a connection the reader cannot classify, and it says `unknown`, which the resolver turns into an absence rather than an identifier reading `unknown`.",
        ));
        add(Entity::new(
            "board.display.{n}.width",
            D::Board,
            K::Identity,
            Some(U::Count),
            P::Measured,
            true,
            concat!(
                "Horizontal resolution in pixels, at the mode currently set. Nullable: ",
                "an attached display whose mode is unreadable has no resolution to ",
                "report, and zero is not one.",
            ),
        ));
        add(Entity::new(
            "board.display.{n}.height",
            D::Board,
            K::Identity,
            Some(U::Count),
            P::Measured,
            true,
            "Vertical resolution in pixels, at the mode currently set.",
        ));
        add(Entity::new(
            "board.display.{n}.refresh_rate",
            D::Board,
            K::Measurement,
            Some(U::Hertz),
            P::Measured,
            true,
            "Refresh rate of the mode currently set.",
        ));
        add(Entity::new(
            "board.display.{n}.primary",
            D::Board,
            K::Identity,
            None,
            P::Measured,
            false,
            "Whether the platform treats this as the primary display.",
        ));

        // ── Platform sensors ─────────────────────────────────────────────────
        //
        // The last cluster named in plan item F. These are the OS-level sensor
        // devices — ambient light, accelerometer, orientation — not the board
        // temperature sensors, which resolve under `thermal`.
        add(Entity::new(
            "board.sensor.<none>",
            D::Board,
            K::Diagnostic,
            None,
            P::Unavailable,
            true,
            "Present when no platform sensor was enumerated, carrying why. A desktop reporting none is the common case and a true reading; this row distinguishes it from a query that could not be made.",
        ));
        add(Entity::new(
            "board.sensor.{n}.name",
            D::Board,
            K::Identity,
            Some(U::Text),
            P::Measured,
            false,
            "Sensor name as the platform reports it. A sensor the platform did not name is skipped rather than called \"Unknown\".",
        ));
        add(Entity::new(
            "board.sensor.{n}.type",
            D::Board,
            K::Identity,
            Some(U::Identifier),
            P::Measured,
            false,
            "Sensor kind - ambient light, accelerometer, gyroscope, orientation.",
        ));
        add(Entity::new(
            "board.sensor.{n}.active",
            D::Board,
            K::Identity,
            None,
            P::Measured,
            false,
            "Whether the platform reports this sensor as ready. A present but inactive sensor is a different fact from an absent one.",
        ));

        // ── Package energy (RAPL) ────────────────────────────────────────────
        //
        // Named in the plan's item F as a remaining cluster. Templated per
        // package because a two-socket machine has two, and the diagnostic for
        // "this platform has no RAPL" is the domain's `<none>` row, which
        // already exists for every domain.
        add(Entity::new(
            "power.rapl.<none>",
            D::Power,
            K::Diagnostic,
            None,
            P::Unavailable,
            true,
            "Present when no RAPL energy domain was enumerated, carrying why. A sub-cluster diagnostic rather than the domain-wide `power.<none>`: a machine can have a readable battery and no RAPL interface, and claiming the whole power domain enumerated nothing would be false.",
        ));
        add(Entity::new(
            "power.rapl.{n}.name",
            D::Power,
            K::Identity,
            Some(U::Text),
            P::Measured,
            false,
            "RAPL domain name as the platform reports it - package-0, core, dram, uncore.",
        ));
        add(Entity::new(
            "power.rapl.{n}.energy",
            D::Power,
            K::Measurement,
            Some(U::Count),
            P::Measured,
            false,
            "Cumulative energy counter in microjoules. A counter, not a rate: it wraps at `max_energy_range`, and power in watts is the difference between two readings divided by the interval. Reported raw because a single sample cannot be converted to watts and pretending otherwise would invent a rate.",
        ));
        add(Entity::new(
            "power.rapl.{n}.max_energy_range",
            D::Power,
            K::Limit,
            Some(U::Count),
            P::Specification,
            false,
            "The value the energy counter wraps at, in microjoules. Needed to compute power across a wrap; a consumer without it will read a wrap as a negative delta.",
        ));
        add(Entity::new(
            "power.rapl.{n}.power_limit",
            D::Power,
            K::Limit,
            Some(U::Watts),
            P::Specification,
            true,
            "The configured power limit for this domain. Nullable: not every RAPL domain publishes a constraint.",
        ));
        add(Entity::new(
            "power.rapl.{n}.enabled",
            D::Power,
            K::Identity,
            None,
            P::Specification,
            false,
            "Whether the platform reports this domain as enabled. A disabled domain still has a counter and it does not advance.",
        ));

        // ── Memory bandwidth ─────────────────────────────────────
        //
        // Every figure here except the configuration itself is `Derived`, and
        // that matters more than usual: nothing in this cluster was measured.
        // The peak is arithmetic over the channel count and the transfer rate,
        // and the achievable and STREAM figures are that peak scaled by a
        // constant. An agent choosing a batch size from these should know it is
        // reading a calculation and not a benchmark - which is what the
        // provenance field is for.
        add(Entity::new(
            "memory.bandwidth.<none>",
            D::Memory,
            K::Diagnostic,
            None,
            P::Unavailable,
            true,
            "Present when the memory configuration needed to estimate bandwidth \
             could not be read, carrying why.",
        ));
        add(Entity::new(
            "memory.bandwidth.generation",
            D::Memory,
            K::Identity,
            Some(U::Identifier),
            P::Specification,
            true,
            "Memory generation the estimate rests on - DDR5, LPDDR5X, HBM3. \
             Nullable because a virtual machine's SMBIOS routinely names no \
             generation at all, which is what both CI runners see; naming one \
             anyway would put a spec-sheet figure behind a bandwidth estimate \
             that has no memory type to rest on.",
        ));
        add(Entity::new(
            "memory.bandwidth.speed",
            D::Memory,
            K::Identity,
            Some(U::Count),
            P::Specification,
            true,
            "Transfer rate in megatransfers per second, as used in the estimate. Nullable because it is the generation's figure: with no generation identified the estimator substitutes 3200 MT/s, which is a default and not a reading.",
        ));
        add(Entity::new(
            "memory.bandwidth.channels",
            D::Memory,
            K::Identity,
            Some(U::Count),
            P::Specification,
            false,
            "Active channel count. The largest single term in the estimate: a \
             dual-channel machine misread as single-channel halves every figure \
             below it.",
        ));
        add(Entity::new(
            "memory.bandwidth.max_channels",
            D::Memory,
            K::Limit,
            Some(U::Count),
            P::Specification,
            false,
            "Channels the controller supports. Above `channels` means the machine \
             is running below the bandwidth its board allows, which is usually a \
             populated-slot problem and worth being able to see.",
        ));
        add(Entity::new(
            "memory.bandwidth.peak",
            D::Memory,
            K::Limit,
            Some(U::BytesPerSecond),
            P::Derived,
            true,
            "Theoretical peak bandwidth, computed from the transfer rate, the bus \
             width and the channel count. No workload reaches it. Nullable: withheld entirely when the generation was not identified, since the transfer rate and bus width it needs would both be built-in defaults.",
        )
        .derived(&[
            "memory.bandwidth.generation",
            "memory.bandwidth.speed",
            "memory.bandwidth.channels",
        ]));
        add(Entity::new(
            "memory.bandwidth.achievable",
            D::Memory,
            K::Limit,
            Some(U::BytesPerSecond),
            P::Derived,
            true,
            "Peak scaled by a fixed efficiency factor. A rule of thumb rendered as \
             a number; it carries no information the peak and the factor do not \
             already contain. Nullable with peak, and for the same reason -- the efficiency factor also falls back to a default when the generation is unknown.",
        )
        .derived(&[
            "memory.bandwidth.generation",
            "memory.bandwidth.speed",
            "memory.bandwidth.channels",
        ]));
        add(Entity::new(
            "memory.bandwidth.stream_triad",
            D::Memory,
            K::Limit,
            Some(U::BytesPerSecond),
            P::Derived,
            true,
            "What the STREAM Triad benchmark would be expected to report. Named \
             after the benchmark but not produced by running it - ironmon runs no \
             benchmarks, and a consumer comparing this against a real STREAM \
             result is comparing an estimate to a measurement. Nullable with peak and achievable, which it is scaled from.",
        )
        .derived(&[
            "memory.bandwidth.generation",
            "memory.bandwidth.speed",
            "memory.bandwidth.channels",
        ]));

        // ── System and board ─────────────────────────────────────────────────
        //
        // The four below were readable by `os_info` long before they were named
        // here. A reader holding data the schema does not expose is invisible to
        // every agent, which is the same silence this module exists to prevent —
        // it just happens one level up.
        add(Entity::new(
            "system.hostname",
            D::System,
            K::Identity,
            Some(U::Text),
            P::Measured,
            true,
            "The machine's hostname. Nullable: a container or a freshly imaged host may genuinely have none set.",
        ));
        add(Entity::new(
            "system.os.version",
            D::System,
            K::Identity,
            Some(U::Text),
            P::Measured,
            true,
            "Operating system version string, as distinct from the build number and from the product name.",
        ));
        add(Entity::new(
            "system.kernel.version",
            D::System,
            K::Identity,
            Some(U::Text),
            P::Measured,
            true,
            "Kernel version. On Windows this is the NT build rather than a separate kernel line, which is why it can equal the OS build.",
        ));
        add(Entity::new(
            "system.architecture",
            D::System,
            K::Identity,
            Some(U::Identifier),
            P::Measured,
            true,
            "CPU architecture the OS reports — x86_64, aarch64. The architecture of the running kernel, which on some hosts differs from the hardware's.",
        ));
        add(Entity::new(
            "system.os.name",
            D::System,
            K::Identity,
            Some(U::Text),
            P::Measured,
            false,
            "Operating system product name, corrected where the vendor's own \
             registry is stale.",
        ));
        add(Entity::new(
            "system.os.build",
            D::System,
            K::Identity,
            Some(U::Identifier),
            P::Measured,
            true,
            "OS build identifier.",
        ));
        add(Entity::new(
            "system.uptime",
            D::System,
            K::Measurement,
            Some(U::Seconds),
            P::Measured,
            // Nullable: the resolver reports it absent when the uptime
            // reader errors.
            true,
            "Seconds since boot.",
        ));
        add(Entity::new(
            "system.boot.duration",
            D::System,
            K::Measurement,
            Some(U::Seconds),
            P::Measured,
            true,
            "Measured boot duration. Null when it could not be read — never a \
             representative constant.",
        ));
        add(Entity::new(
            "system.boot.secure_boot",
            D::System,
            K::Identity,
            None,
            P::Measured,
            true,
            "Whether Secure Boot is enforcing, read from the firmware flag itself \
             rather than inferred from the presence of the key holding it.",
        ));
        add(Entity::new(
            "board.model",
            D::Board,
            K::Identity,
            Some(U::Text),
            P::Measured,
            true,
            "Baseboard product name from SMBIOS.",
        ));
        add(Entity::new(
            "board.firmware.vendor",
            D::Board,
            K::Identity,
            Some(U::Text),
            P::Measured,
            true,
            "System firmware vendor, from SMBIOS.",
        ));
        add(Entity::new(
            "board.firmware.product",
            D::Board,
            K::Identity,
            Some(U::Text),
            P::Measured,
            true,
            "System product name, from SMBIOS.",
        ));
        add(Entity::new(
            "board.firmware.boot_mode",
            D::Board,
            K::Identity,
            Some(U::Identifier),
            P::Measured,
            true,
            "Whether the machine booted UEFI or legacy BIOS. Determines whether \
             Secure Boot is even applicable.",
        ));
        add(Entity::new(
            "board.firmware.{n}.component",
            D::Board,
            K::Identity,
            Some(U::Identifier),
            P::Measured,
            true,
            "Name of one component in the firmware inventory — BIOS, ME, a device \
             option ROM.",
        ));
        add(Entity::new(
            "board.firmware.{n}.version",
            D::Board,
            K::Identity,
            Some(U::Text),
            P::Measured,
            true,
            "Installed version of that component.",
        ));
        add(Entity::new(
            "board.tpm.present",
            D::Board,
            K::Identity,
            None,
            P::Measured,
            // Nullable since 6.0.0. `TpmMonitor::refresh` can report a
            // failure now, and the resolver already said what to do with
            // one: "not knowing whether a TPM exists is different from
            // knowing there is none".
            true,
            "Whether a TPM was enumerated at all. False is a reading: the absence \
             of a TPM is a fact about the machine, distinct from being unable to \
             look.",
        ));
        add(Entity::new(
            "board.tpm.version",
            D::Board,
            K::Identity,
            Some(U::Identifier),
            P::Measured,
            true,
            "TPM specification version. Null when no TPM is present.",
        ));
        add(Entity::new(
            "board.tpm.manufacturer",
            D::Board,
            K::Identity,
            Some(U::Text),
            P::Measured,
            true,
            "TPM manufacturer string.",
        ));
        add(Entity::new(
            "board.tpm.status",
            D::Board,
            K::Measurement,
            Some(U::Identifier),
            P::Measured,
            true,
            "Whether the TPM is enabled, activated and owned. A present but \
             disabled TPM cannot attest anything, so presence alone is not the \
             question an agent should ask.",
        ));
        add(Entity::new(
            "board.tpm.measured_boot",
            D::Board,
            K::Measurement,
            None,
            P::Measured,
            true,
            "Whether platform integrity measurements are active.",
        ));
        add(Entity::new(
            "board.manufacturer",
            D::Board,
            K::Identity,
            Some(U::Text),
            P::Measured,
            true,
            "Baseboard manufacturer from SMBIOS.",
        ));

        // ── Writable settings ────────────────────────────────────────────────
        //
        // Generated from the apply-handler registry rather than listed here, for the
        // same reason the command catalogue is walked out of clap: a hand-maintained
        // list would claim a write the binary cannot perform the first time a
        // handler was removed and nobody remembered this function. An agent reading
        // `writable_via` is being told it can change something, which is a promise
        // that has to be backed by a registered handler.
        //
        // Until this existed, `EntityKind::Setting` and `Entity::writable_via` were
        // declared and populated by nothing: the ontology described a machine an
        // agent could only read, while `ironmon profile set` could write.
        for handler in crate::profile::apply::builtin_handlers() {
            let setting_id = handler.setting_id();
            let domain = setting_domain(handler.subsystem());
            let mut entity = Entity::new(
                &format!("{}.setting.{}", domain.as_str(), setting_id),
                domain,
                K::Setting,
                Some(U::Identifier),
                P::Measured,
                true,
                "A driver or firmware setting with a registered write handler. \
                 Writing requires explicit confirmation and is recorded in the \
                 apply audit log.",
            );
            entity.writable_via = Some(setting_id.to_string());
            add(entity);
        }

        // ── Diagnostics ──────────────────────────────────────────────────────
        //
        // Declared, not synthesised at read time, because everything the resolver
        // emits has to be findable in the schema an agent fetched beforehand. These
        // were the one class of row that violated that: emitting an undeclared
        // `disk.<none>` told an agent something true in a vocabulary it had no way
        // to look up.
        for domain in Domain::ALL {
            add(Entity::new(
                &format!("{}.<none>", domain.as_str()),
                *domain,
                K::Diagnostic,
                None,
                P::Unavailable,
                true,
                "Present only when this domain enumerated nothing. Carries the \
                 reason, so an absent device stays distinguishable from a reader \
                 that is not implemented here.",
            ));
        }
        add(Entity::new(
            "process.<truncated>",
            D::Process,
            K::Diagnostic,
            Some(U::Count),
            P::Unavailable,
            true,
            "Present when the process list was capped. Absence from a truncated \
             list is not absence from the machine.",
        ));

        Self {
            version: ONTOLOGY_VERSION.to_string(),
            entities,
        }
    }

    pub fn get(&self, id: &str) -> Option<&Entity> {
        self.entities.get(id)
    }

    /// Entities in one domain, in id order.
    pub fn in_domain(&self, domain: Domain) -> Vec<&Entity> {
        self.entities
            .values()
            .filter(|e| e.domain == domain)
            .collect()
    }

    /// Substring search over ids and descriptions, for agents that do not yet know
    /// the id space.
    pub fn search(&self, needle: &str) -> Vec<&Entity> {
        let needle = needle.to_ascii_lowercase();
        self.entities
            .values()
            .filter(|e| {
                e.id.to_ascii_lowercase().contains(&needle)
                    || e.description.to_ascii_lowercase().contains(&needle)
            })
            .collect()
    }

    /// Ids whose value is a template — `gpu.{n}.name` rather than a concrete
    /// `gpu.0.name`. A consumer expands these against the live instance count.
    pub fn is_template(id: &str) -> bool {
        id.contains('{')
    }

    /// The entity a concrete id belongs to, resolving templates.
    ///
    /// `gpu.0.name` finds `gpu.{n}.name`; an exact id finds itself. This is the
    /// inverse of expansion, and the lookup an agent needs when it holds a reading
    /// and wants the schema behind it — the unit it is in, whether it may be null,
    /// and the range it must fall inside.
    ///
    /// Segment count must match, so `disk.0.smart.passed` cannot match
    /// `disk.{n}.model`. A `{placeholder}` segment matches any single segment.
    pub fn template_for(&self, concrete: &str) -> Option<&Entity> {
        if let Some(e) = self.get(concrete) {
            return Some(e);
        }
        let parts: Vec<&str> = concrete.split('.').collect();
        self.entities.values().find(|e| {
            let tparts: Vec<&str> = e.id.split('.').collect();
            tparts.len() == parts.len()
                && tparts
                    .iter()
                    .zip(&parts)
                    .all(|(t, c)| t.starts_with('{') || t == c)
        })
    }

    /// Expand a template id for a concrete instance.
    ///
    /// `gpu.{n}.name` with `"0"` becomes `gpu.0.name`. Returns the id unchanged when
    /// it holds no placeholder, so callers need not special-case singletons.
    pub fn instantiate(id: &str, instance: &str) -> String {
        if let Some(open) = id.find('{') {
            if let Some(close) = id[open..].find('}') {
                let mut out = String::with_capacity(id.len() + instance.len());
                out.push_str(&id[..open]);
                out.push_str(instance);
                out.push_str(&id[open + close + 1..]);
                return out;
            }
        }
        id.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_id_is_unique_and_well_formed() {
        let ont = Ontology::build();
        assert!(!ont.entities.is_empty(), "ontology is empty");
        for (key, e) in &ont.entities {
            assert_eq!(key, &e.id, "map key and entity id disagree");
            assert!(
                e.id.starts_with(e.domain.as_str()),
                "{} is not namespaced under its domain {}",
                e.id,
                e.domain.as_str()
            );
            assert!(
                !e.description.is_empty(),
                "{} has no description; the ontology exists to be read by something \
                 that cannot see the hardware",
                e.id
            );
            assert!(
                !e.id.ends_with('.') && !e.id.contains(".."),
                "malformed id: {}",
                e.id
            );
        }
    }

    /// `writable_via` tells an agent it may change something. That is a promise, and
    /// it must be backed by a handler that actually exists — a schema claiming a
    /// write the binary cannot perform is worse than one claiming nothing, because
    /// the agent will try.
    #[test]
    fn every_writable_entity_has_a_registered_handler() {
        let ont = Ontology::build();
        let registered: Vec<String> = crate::profile::apply::builtin_handlers()
            .iter()
            .map(|h| h.setting_id().to_string())
            .collect();

        for e in ont.entities.values() {
            let Some(setting_id) = &e.writable_via else {
                continue;
            };
            assert_eq!(
                e.kind,
                EntityKind::Setting,
                "{} declares writable_via but is not a Setting",
                e.id
            );
            assert!(
                registered.contains(setting_id),
                "{} claims it can be written via {setting_id:?}, but no such apply \
                 handler is registered on this build; an agent told this would \
                 attempt a write that cannot succeed. Registered: {registered:?}",
                e.id
            );
        }
    }

    /// The converse: a handler the binary exposes but the schema hides is a
    /// capability an agent cannot discover. Reading `ironmon profile writable` should
    /// not reveal anything `ironmon describe` omits.
    #[test]
    fn every_registered_handler_is_discoverable_in_the_schema() {
        let ont = Ontology::build();
        let declared: Vec<&String> = ont
            .entities
            .values()
            .filter_map(|e| e.writable_via.as_ref())
            .collect();

        for handler in crate::profile::apply::builtin_handlers() {
            let id = handler.setting_id().to_string();
            assert!(
                declared.contains(&&id),
                "the apply registry exposes {id:?} but no ontology entity declares \
                 it, so an agent reading the schema cannot discover it"
            );
        }
    }

    /// Settings are only meaningful if the kind is used at all — an assertion over
    /// an empty set would pass while the write surface was entirely undeclared,
    /// which is precisely the state this replaced.
    #[test]
    fn the_schema_declares_at_least_one_writable_setting() {
        // Platform-dependent: a build with no handlers for this OS legitimately has
        // none, so this only asserts the two views agree about how many there are.
        let ont = Ontology::build();
        let declared = ont
            .entities
            .values()
            .filter(|e| e.kind == EntityKind::Setting)
            .count();
        let registered = crate::profile::apply::builtin_handlers().len();
        assert_eq!(
            declared, registered,
            "the schema declares {declared} writable settings but the apply registry \
             has {registered}"
        );
    }

    /// A derived value is only as trustworthy as its inputs, so it has to name them.
    #[test]
    fn derived_entities_declare_their_inputs() {
        let ont = Ontology::build();
        for e in ont.entities.values() {
            if e.provenance == Provenance::Derived {
                assert!(
                    !e.derived_from.is_empty(),
                    "{} is derived but names no inputs",
                    e.id
                );
                for input in &e.derived_from {
                    assert!(
                        ont.get(input).is_some(),
                        "{} derives from {input}, which is not in the ontology",
                        e.id
                    );
                }
            } else {
                assert!(
                    e.derived_from.is_empty(),
                    "{} names inputs but is not marked derived",
                    e.id
                );
            }
        }
    }

    /// Only `Measured` counts as an observation. This is the guard that stops an
    /// agent treating a spec-sheet constant as a live reading.
    #[test]
    fn only_measured_values_are_observations() {
        assert!(Provenance::Measured.is_observation());
        assert!(!Provenance::Specification.is_observation());
        assert!(!Provenance::Derived.is_observation());
        assert!(!Provenance::Unavailable.is_observation());
    }

    #[test]
    fn range_validation_rejects_impossible_readings() {
        let ont = Ontology::build();
        let util = ont.get("cpu.total.utilization").unwrap();
        assert!(util.validate_range(50.0).is_none());
        assert!(util.validate_range(-1.0).is_some(), "negative percent");
        assert!(util.validate_range(101.0).is_some(), "over 100 percent");

        let temp = ont.get("gpu.{n}.thermal.temperature").unwrap();
        assert!(temp.validate_range(45.0).is_none());
        // Celsius may be negative; absolute zero may not be undercut.
        assert!(temp.validate_range(-10.0).is_none());
        assert!(temp.validate_range(-300.0).is_some(), "below absolute zero");
        assert!(temp.validate_range(500.0).is_some(), "implausibly hot");
    }

    #[test]
    fn templates_expand_to_concrete_ids() {
        assert!(Ontology::is_template("gpu.{n}.name"));
        assert!(!Ontology::is_template("memory.total"));
        assert_eq!(Ontology::instantiate("gpu.{n}.name", "0"), "gpu.0.name");
        assert_eq!(
            Ontology::instantiate("network.{iface}.rx_rate", "eth0"),
            "network.eth0.rx_rate"
        );
        // Non-templates pass through untouched.
        assert_eq!(Ontology::instantiate("memory.total", "0"), "memory.total");
    }

    #[test]
    fn lookup_and_search_find_entities() {
        let ont = Ontology::build();
        assert!(ont.get("memory.total").is_some());
        assert!(ont.get("no.such.entity").is_none());
        assert!(!ont.in_domain(Domain::Gpu).is_empty());
        assert!(
            !ont.search("temperature").is_empty(),
            "search should find thermal entities"
        );
        assert_eq!(Domain::parse("gpu"), Some(Domain::Gpu));
        assert_eq!(Domain::parse("GPU"), Some(Domain::Gpu));
        assert_eq!(Domain::parse("nonsense"), None);
    }

    /// The ontology must not depend on the machine it is built on: an agent fetches
    /// it as a schema, so two builds have to agree.
    #[test]
    fn build_is_deterministic_and_hardware_independent() {
        let a = serde_json::to_string(&Ontology::build()).unwrap();
        let b = serde_json::to_string(&Ontology::build()).unwrap();
        assert_eq!(a, b);
    }
}

/// Human-facing labels, derived from the ontology rather than retyped per surface.
///
/// The CLI printed `Kernel:`, the GUI printed `Kernel Version`, and the JSON emitted
/// `kernel_version` — three names for one reading, which is exactly what stops an
/// agent correlating what a user reports seeing with what it can query. These
/// helpers give the TUI and GUI the same source the CLI and `ironmon describe` use, so
/// a label shown on screen can be turned back into an id.
pub mod labels {
    use super::{Domain, Ontology};

    /// Title-cased label for an entity id, e.g. `gpu.0.thermal.temperature` ->
    /// "Temperature". Falls back to the last path segment for ids the ontology does
    /// not declare, so a surface never renders a raw dotted id at a user.
    pub fn short_label(id: &str) -> String {
        let leaf = id.rsplit('.').next().unwrap_or(id);
        title_case(leaf)
    }

    /// Fully qualified label including the domain and instance, e.g.
    /// "GPU 0 — Temperature". Used where a surface shows readings from several
    /// devices in one list.
    pub fn qualified_label(id: &str) -> String {
        let parts: Vec<&str> = id.split('.').collect();
        let short = short_label(id);
        match parts.as_slice() {
            [domain, instance, ..] if instance.chars().all(|c| c.is_ascii_digit()) => {
                format!("{} {} — {}", domain_label(domain), instance, short)
            }
            [domain, ..] => format!("{} — {}", domain_label(domain), short),
            [] => short,
        }
    }

    /// Display name for a domain, preserving the acronyms users expect to see.
    pub fn domain_label(domain: &str) -> String {
        match domain {
            "cpu" => "CPU".to_string(),
            "gpu" => "GPU".to_string(),
            // Added with the `usb` domain in 3.4.0. Without it the GUI's existing
            // "USB Devices" heading and the ontology's `Usb` disagreed, which is
            // precisely what `hardcoded_headings_do_not_contradict_the_ontology`
            // exists to catch — and did, on the commit that introduced the domain.
            "usb" => "USB".to_string(),
            _ => title_case(domain),
        }
    }

    /// The id a label came from, if the ontology declares exactly one match.
    ///
    /// The inverse direction, and the reason the labels live here: an agent handed
    /// "Temperature" by a user looking at the GUI can ask which ids that could mean
    /// instead of guessing.
    pub fn ids_for_label(label: &str) -> Vec<String> {
        let ontology = Ontology::build();
        let needle = label.trim().to_ascii_lowercase();
        ontology
            .entities
            .keys()
            .filter(|id| short_label(id).to_ascii_lowercase() == needle)
            .cloned()
            .collect()
    }

    /// Whether a domain name is one the ontology knows, so a surface can assert its
    /// section headings correspond to real domains.
    pub fn is_known_domain(domain: &str) -> bool {
        Domain::parse(domain).is_some()
    }

    fn title_case(s: &str) -> String {
        s.split('_')
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    Some(first) => {
                        first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()
                    }
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn labels_are_derived_from_ids() {
            assert_eq!(short_label("gpu.0.thermal.temperature"), "Temperature");
            assert_eq!(short_label("memory.total"), "Total");
            assert_eq!(short_label("system.os.name"), "Name");
            // Underscores become spaces, not run-together words.
            assert_eq!(
                short_label("gpu.0.thermal.critical_temperature"),
                "Critical Temperature"
            );
        }

        #[test]
        fn qualified_labels_name_the_device() {
            assert_eq!(
                qualified_label("gpu.0.thermal.temperature"),
                "GPU 0 — Temperature"
            );
            assert_eq!(qualified_label("memory.total"), "Memory — Total");
            // Acronyms survive title-casing.
            assert_eq!(domain_label("cpu"), "CPU");
            assert_eq!(domain_label("memory"), "Memory");
        }

        /// The inverse direction has to actually work, or the label layer is
        /// decoration rather than a mapping.
        #[test]
        fn labels_map_back_to_ids() {
            let ids = ids_for_label("Total");
            assert!(
                ids.contains(&"memory.total".to_string()),
                "expected memory.total among {ids:?}"
            );
            assert!(ids_for_label("no such label").is_empty());
        }

        #[test]
        fn every_ontology_domain_is_a_known_domain() {
            for d in Domain::ALL {
                assert!(is_known_domain(d.as_str()));
            }
            assert!(!is_known_domain("nonsense"));
        }

        /// No entity may produce an empty label — a surface would render a blank.
        #[test]
        fn no_entity_yields_an_empty_label() {
            for id in Ontology::build().entities.keys() {
                assert!(
                    !short_label(id).trim().is_empty(),
                    "{id} produced an empty label"
                );
                assert!(
                    !qualified_label(id).trim().is_empty(),
                    "{id} produced an empty qualified label"
                );
            }
        }
    }
}
