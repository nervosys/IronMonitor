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
    board::resolve(out);
    cpu::resolve(out);
    gpu::resolve(out);
    memory::resolve(out);
    network::resolve(out);
    npu::resolve(out);
    peripherals::resolve(out);
    storage::resolve(out);
}
