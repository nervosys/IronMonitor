//! Memory module detail (serial, bank, ranks, form factor), topology analysis, EDAC rows and NUMA detail.

#[allow(unused_imports)]
use super::super::resolve::*;
#[allow(unused_imports)]
use super::super::{Domain as D, Entity, EntityKind as K, Provenance as P, Unit as U};

pub(super) fn declare(_add: &mut dyn FnMut(Entity)) {}

pub(super) fn resolve(_out: &mut Vec<Reading>) {}
