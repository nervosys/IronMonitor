//! Projecting a [`Snapshot`] into table rows.
//!
//! This is where the crate's `Option` discipline either survives into the store
//! or is quietly lost, so it is written to make losing it hard and tested to
//! make losing it loud.
//!
//! Three distinctions have to come through intact, and all three are invisible
//! if a reading arrives as a number:
//!
//! - **No accelerator** versus **an accelerator that did not answer.** A
//!   machine with no GPU produces one row with the GPU columns null. A machine
//!   whose card failed to read produces a row that *names the card* and leaves
//!   its metrics null — [`Snapshot::gpu_dynamic`] is index-aligned with
//!   `gpu_static` and keeps a failed device's slot, precisely so this stays
//!   expressible. The two look identical the moment either becomes a zero.
//! - **Idle** versus **unread.** `GpuDynamicInfo::utilization` is already
//!   `Option<u8>` because two backends used to return a literal `0` having read
//!   nothing; that history is in [`crate::gpu`]. Nothing here re-introduces it.
//! - **No interfaces up** versus **no rate established yet.**
//!   [`Snapshot::total_rx_rate`] returns `None` rather than `0.0` when no
//!   interface has a rate, on the stated grounds that "a total over nothing
//!   measured is not a measurement of nothing".
//!
//! # Units
//!
//! The table stores bytes; [`crate::core::memory::RamInfo`] stores kibibytes.
//! The conversion happens here, once, and is the only place it happens.

use crate::pipeline::Snapshot;

use super::HostId;

/// One row of the `host_metrics` table.
///
/// A plain struct rather than an Arrow builder, so the projection can be tested
/// without a columnar library in the way. Conversion to Arrow is a separate,
/// mechanical step.
#[derive(Debug, Clone, PartialEq)]
pub struct MetricRow {
    pub host_id: String,
    /// Unix microseconds, which is Iceberg's `timestamptz` resolution. The
    /// snapshot carries seconds, so this is a widening rather than a rounding.
    pub collected_at_us: i64,
    pub generation: i64,

    pub cpu_utilization: Option<f64>,
    pub cpu_cores: Option<i32>,

    pub memory_used_bytes: Option<i64>,
    pub memory_total_bytes: Option<i64>,
    pub swap_used_bytes: Option<i64>,
    pub swap_total_bytes: Option<i64>,

    pub gpu_index: Option<i32>,
    pub gpu_name: Option<String>,
    pub gpu_utilization: Option<f64>,
    pub gpu_memory_used_bytes: Option<i64>,
    pub gpu_memory_total_bytes: Option<i64>,
    pub gpu_temperature_c: Option<f64>,
    pub gpu_power_mw: Option<i64>,

    pub net_rx_bytes_per_sec: Option<f64>,
    pub net_tx_bytes_per_sec: Option<f64>,

    pub collect_us: Option<i64>,
}

/// Kibibytes to bytes, refusing to invent a total.
///
/// `RamInfo::total` and `used` are bare `u64` rather than `Option`, so a failed
/// read is not expressible at the field level — only at the `Option<MemoryStats>`
/// above it. A total of zero is not a plausible reading from a machine that is
/// running this program, so it is treated as the absence of one rather than
/// passed through as a measurement. That is a judgement, and it is made here
/// rather than left to every consumer of the table.
fn kib_to_bytes(kib: u64) -> Option<i64> {
    if kib == 0 {
        return None;
    }
    i64::try_from(kib.saturating_mul(1024)).ok()
}

/// Project one snapshot into the rows it contributes.
///
/// **Always at least one row**, even on a machine with no accelerators: the
/// host-level metrics are the reason the row exists, and a fleet whose GPU-less
/// members simply vanish from the table cannot answer questions about the fleet.
///
/// On a machine with accelerators there is one row per card, carrying the
/// host-level columns repeated. See [`super::schema`] for why the layout is flat
/// and what that repetition costs a careless `SUM`.
pub fn rows_from_snapshot(snapshot: &Snapshot, host: &HostId) -> Vec<MetricRow> {
    let base = MetricRow {
        host_id: host.as_str().to_owned(),
        collected_at_us: (snapshot.collected_at as i64).saturating_mul(1_000_000),
        generation: snapshot.generation as i64,

        cpu_utilization: snapshot.cpu_utilization().map(f64::from),
        cpu_cores: snapshot
            .cpu
            .as_ref()
            .and_then(|c| i32::try_from(c.cores.len()).ok()),

        memory_used_bytes: snapshot
            .memory
            .as_ref()
            .and_then(|m| kib_to_bytes(m.ram.used)),
        memory_total_bytes: snapshot
            .memory
            .as_ref()
            .and_then(|m| kib_to_bytes(m.ram.total)),
        swap_used_bytes: snapshot
            .memory
            .as_ref()
            .and_then(|m| m.swap.used)
            .and_then(kib_to_bytes),
        swap_total_bytes: snapshot
            .memory
            .as_ref()
            .and_then(|m| m.swap.total)
            .and_then(kib_to_bytes),

        gpu_index: None,
        gpu_name: None,
        gpu_utilization: None,
        gpu_memory_used_bytes: None,
        gpu_memory_total_bytes: None,
        gpu_temperature_c: None,
        gpu_power_mw: None,

        net_rx_bytes_per_sec: snapshot.total_rx_rate(),
        net_tx_bytes_per_sec: snapshot.total_tx_rate(),

        collect_us: i64::try_from(snapshot.collect_us).ok(),
    };

    if snapshot.gpu_static.is_empty() {
        return vec![base];
    }

    snapshot
        .gpu_static
        .iter()
        .enumerate()
        .map(|(slot, static_info)| {
            let mut row = base.clone();
            // The card is present, so it is named even when nothing about its
            // state could be read. `name` describes the hardware; `uuid` would
            // identify the unit and is deliberately not carried.
            row.gpu_index = i32::try_from(static_info.index).ok();
            row.gpu_name = Some(static_info.name.clone());

            // Index-aligned by contract, but a shorter vector is treated as
            // "not read" rather than panicking: a partial snapshot is a thing
            // that can reach here, and losing the whole tick over it would be
            // the wrong trade.
            if let Some(Some(dynamic)) = snapshot.gpu_dynamic.get(slot) {
                row.gpu_utilization = dynamic.utilization.map(f64::from);
                row.gpu_memory_used_bytes = dynamic.memory.used.and_then(|b| i64::try_from(b).ok());
                row.gpu_memory_total_bytes =
                    dynamic.memory.total.and_then(|b| i64::try_from(b).ok());
                row.gpu_temperature_c = dynamic.thermal.temperature.map(f64::from);
                row.gpu_power_mw = dynamic.power.draw.map(i64::from);
            }
            row
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::{
        GpuClocks, GpuDynamicInfo, GpuEngines, GpuMemory, GpuPower, GpuStaticInfo, GpuThermal,
        GpuVendor, PcieLinkInfo,
    };

    fn host() -> HostId {
        HostId::rotate(Some("test-host"))
    }

    fn card(index: usize, name: &str) -> GpuStaticInfo {
        GpuStaticInfo {
            index,
            vendor: GpuVendor::Nvidia,
            name: name.to_owned(),
            pci_bus_id: None,
            uuid: Some("GPU-deadbeef-0000-0000-0000-000000000000".to_owned()),
            vbios_version: None,
            driver_version: None,
            compute_capability: None,
            shader_cores: None,
            num_engines: None,
            integrated: false,
            l2_cache: None,
        }
    }

    fn reading(utilization: Option<u8>, temperature: Option<i32>) -> GpuDynamicInfo {
        GpuDynamicInfo {
            utilization,
            memory: GpuMemory {
                total: Some(8 * 1024 * 1024 * 1024),
                used: Some(1024 * 1024 * 1024),
                free: None,
                utilization: None,
            },
            clocks: GpuClocks {
                graphics: None,
                graphics_max: None,
                memory: None,
                memory_max: None,
                sm: None,
                video: None,
            },
            power: GpuPower {
                draw: Some(210_000),
                limit: None,
                default_limit: None,
                usage_percent: None,
            },
            thermal: GpuThermal {
                temperature,
                max_temperature: None,
                critical_temperature: None,
                fan_speed: None,
                fan_rpm: None,
            },
            pcie: PcieLinkInfo {
                current_gen: None,
                max_gen: None,
                current_width: None,
                max_width: None,
                current_speed: None,
                max_speed: None,
                tx_throughput: None,
                rx_throughput: None,
            },
            engines: GpuEngines {
                graphics: None,
                compute: None,
                encoder: None,
                decoder: None,
                copy: None,
                vendor_specific: Vec::new(),
            },
            processes: Vec::new(),
        }
    }

    /// A machine with no accelerators must still appear in the table.
    #[test]
    fn a_gpu_less_host_still_produces_a_row() {
        let snapshot = Snapshot {
            generation: 7,
            collected_at: 1_700_000_000,
            ..Default::default()
        };

        let rows = rows_from_snapshot(&snapshot, &host());
        assert_eq!(rows.len(), 1, "a fleet member with no GPU must not vanish");
        assert_eq!(rows[0].generation, 7);
        assert!(rows[0].gpu_index.is_none());
        assert!(rows[0].gpu_utilization.is_none());
    }

    /// One row per card, so a per-card question has a per-card row to answer it.
    #[test]
    fn each_accelerator_gets_its_own_row() {
        let snapshot = Snapshot {
            generation: 1,
            gpu_static: vec![
                card(0, "NVIDIA A"),
                card(1, "NVIDIA B"),
                card(2, "NVIDIA C"),
            ],
            gpu_dynamic: vec![
                Some(reading(Some(23), Some(49))),
                Some(reading(Some(0), Some(45))),
                Some(reading(None, None)),
            ],
            ..Default::default()
        };

        let rows = rows_from_snapshot(&snapshot, &host());
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].gpu_name.as_deref(), Some("NVIDIA A"));
        assert_eq!(rows[0].gpu_utilization, Some(23.0));
        assert_eq!(rows[0].gpu_temperature_c, Some(49.0));
    }

    /// **The distinction this module exists for.** An idle card reports zero; an
    /// unread card reports nothing. They must not arrive as the same value.
    #[test]
    fn an_idle_card_and_an_unread_card_are_different_rows() {
        let snapshot = Snapshot {
            gpu_static: vec![card(0, "idle"), card(1, "unread")],
            gpu_dynamic: vec![Some(reading(Some(0), Some(40))), Some(reading(None, None))],
            ..Default::default()
        };

        let rows = rows_from_snapshot(&snapshot, &host());
        assert_eq!(
            rows[0].gpu_utilization,
            Some(0.0),
            "a card that reported 0% is a measurement and must be stored as one"
        );
        assert_eq!(
            rows[1].gpu_utilization, None,
            "a card that reported nothing must be null, or it averages like an idle one"
        );
    }

    /// A card that failed to answer keeps its slot and its name.
    #[test]
    fn a_card_that_did_not_answer_is_still_named() {
        let snapshot = Snapshot {
            gpu_static: vec![card(0, "NVIDIA A"), card(1, "NVIDIA B")],
            // The second device failed entirely; the slot is kept by contract.
            gpu_dynamic: vec![Some(reading(Some(50), Some(60))), None],
            ..Default::default()
        };

        let rows = rows_from_snapshot(&snapshot, &host());
        assert_eq!(
            rows.len(),
            2,
            "a failed device must not drop out of the tick"
        );
        assert_eq!(rows[1].gpu_name.as_deref(), Some("NVIDIA B"));
        assert_eq!(rows[1].gpu_index, Some(1));
        assert_eq!(
            rows[1].gpu_utilization, None,
            "present but unread is null, not zero"
        );
        assert_eq!(rows[1].gpu_memory_used_bytes, None);
    }

    /// The GPU's UUID identifies a unit and must never reach the table.
    #[test]
    fn no_hardware_identifier_reaches_a_row() {
        let snapshot = Snapshot {
            gpu_static: vec![card(0, "NVIDIA A")],
            gpu_dynamic: vec![Some(reading(Some(10), Some(40)))],
            ..Default::default()
        };

        let rows = rows_from_snapshot(&snapshot, &host());
        let rendered = format!("{:?}", rows[0]);
        assert!(
            !rendered.contains("deadbeef"),
            "the card's UUID identifies one unit and is not published; row was {rendered}"
        );
    }

    /// A truncated snapshot must not panic and must not invent readings.
    #[test]
    fn a_shorter_dynamic_vector_reads_as_unmeasured() {
        let snapshot = Snapshot {
            gpu_static: vec![card(0, "a"), card(1, "b")],
            // Deliberately violating the index-alignment contract.
            gpu_dynamic: vec![Some(reading(Some(10), Some(40)))],
            ..Default::default()
        };

        let rows = rows_from_snapshot(&snapshot, &host());
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].gpu_utilization, None);
    }

    /// Every row carries the host, or the table cannot be grouped.
    #[test]
    fn every_row_carries_the_host_id() {
        let id = host();
        let snapshot = Snapshot {
            gpu_static: vec![card(0, "a"), card(1, "b")],
            gpu_dynamic: vec![None, None],
            ..Default::default()
        };

        let rows = rows_from_snapshot(&snapshot, &id);
        assert!(rows.iter().all(|r| r.host_id == id.as_str()));
    }

    /// Kibibytes become bytes exactly once.
    #[test]
    fn memory_is_converted_from_kib_to_bytes() {
        use crate::core::memory::{MemoryStats, RamInfo, SwapInfo};

        let snapshot = Snapshot {
            memory: Some(MemoryStats {
                ram: RamInfo {
                    total: 1024,
                    used: 512,
                    free: 512,
                    buffers: None,
                    cached: None,
                    shared: None,
                    lfb: None,
                },
                swap: SwapInfo {
                    total: Some(2048),
                    used: Some(0),
                    cached: None,
                },
                emc: None,
                iram: None,
            }),
            ..Default::default()
        };

        let rows = rows_from_snapshot(&snapshot, &host());
        assert_eq!(rows[0].memory_total_bytes, Some(1024 * 1024));
        assert_eq!(rows[0].memory_used_bytes, Some(512 * 1024));
        assert_eq!(rows[0].swap_total_bytes, Some(2048 * 1024));
        assert_eq!(
            rows[0].swap_used_bytes, None,
            "zero swap used is indistinguishable from unread at this field and is \
             not asserted as a measurement"
        );
    }

    /// Memory that was never read must not become zero bytes of RAM.
    #[test]
    fn unread_memory_stays_null() {
        let snapshot = Snapshot {
            memory: None,
            ..Default::default()
        };
        let rows = rows_from_snapshot(&snapshot, &host());
        assert_eq!(rows[0].memory_total_bytes, None);
        assert_eq!(rows[0].memory_used_bytes, None);
    }
}
