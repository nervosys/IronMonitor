//! The full hardware inventory: everything the reader modules collect that the
//! base schema did not publish, and the network context it had no reader for.
//!
//! One file per area, each declaring its entities and resolving them, so the
//! schema and its resolver stay side by side. The rules are the base
//! ontology's: a value that was not read is `unavailable` with a reason, never
//! a zero or a placeholder; a firmware-declared value is `specification`, not
//! `measured`; and a domain that enumerates nothing says so.
//!
//! Identifying values -- serial numbers, UUIDs, MAC and IP addresses, SSIDs and
//! BSSIDs -- are published like any other since ontology 1.1. Until then serials
//! were deliberately left out; the inventory is meant to be complete.

use super::resolve::{Reading, Scope};
use super::Entity;

mod board;
mod cpu;
mod gpu;
mod memory;
mod network;
mod npu;
mod peripherals;
mod storage;

pub(super) fn declare(add: &mut dyn FnMut(Entity)) {
    board::declare(add);
    cpu::declare(add);
    gpu::declare(add);
    memory::declare(add);
    network::declare(add);
    npu::declare(add);
    peripherals::declare(add);
    storage::declare(add);
}

/// An area's reader, with the id segments it emits.
type Area = (&'static [&'static str], fn(&mut Vec<Reading>));

/// Run the areas `scope` wants. Each area names the id segments it emits, so an
/// area nothing in the scope reads from is not run at all.
///
/// The lists were taken from a Windows snapshot and from the `"<segment>."` literals in
/// each area's source, which cover every platform. An area that emits a segment missing
/// from its list would drop those rows from a scoped `get`; the full snapshot is
/// unaffected.
pub(super) fn resolve(out: &mut Vec<Reading>, scope: &Scope) {
    // The areas share nothing, and most of their time is spent waiting on
    // PowerShell, WMI or sysfs, so they run side by side: sequentially a
    // snapshot took most of a minute on Windows. Results are appended in a
    // fixed order, so the output does not depend on which thread finished
    // first.
    let areas: [Area; 8] = [
        (&["board", "power"], board::resolve),
        (&["cpu", "system"], cpu::resolve),
        (&["gpu", "pci", "system"], gpu::resolve),
        (&["memory"], memory::resolve),
        (&["network"], network::resolve),
        (&["npu"], npu::resolve),
        (&["board", "network", "usb"], peripherals::resolve),
        (&["disk"], storage::resolve),
    ];
    let wanted: Vec<fn(&mut Vec<Reading>)> = areas
        .iter()
        .filter(|(emits, _)| scope.wants(emits))
        .map(|(_, area)| *area)
        .collect();
    let results: Vec<Vec<Reading>> = std::thread::scope(|threads| {
        let handles: Vec<_> = wanted
            .into_iter()
            .map(|area| {
                threads.spawn(move || {
                    let mut rows = Vec::new();
                    area(&mut rows);
                    rows
                })
            })
            .collect();
        // A reader that panics loses its own area, not the snapshot.
        handles
            .into_iter()
            .map(|h| h.join().unwrap_or_default())
            .collect()
    });
    for rows in results {
        out.extend(rows);
    }
}
