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

use super::resolve::Reading;
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

pub(super) fn resolve(out: &mut Vec<Reading>) {
    // The areas share nothing, and most of their time is spent waiting on
    // PowerShell, WMI or sysfs, so they run side by side: sequentially a
    // snapshot took most of a minute on Windows. Results are appended in a
    // fixed order, so the output does not depend on which thread finished
    // first.
    let areas: [fn(&mut Vec<Reading>); 8] = [
        board::resolve,
        cpu::resolve,
        gpu::resolve,
        memory::resolve,
        network::resolve,
        npu::resolve,
        peripherals::resolve,
        storage::resolve,
    ];
    let results: Vec<Vec<Reading>> = std::thread::scope(|scope| {
        let handles: Vec<_> = areas
            .iter()
            .map(|area| {
                scope.spawn(move || {
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
