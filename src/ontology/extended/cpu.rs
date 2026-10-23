//! CPU frequency policy, idle states, turbo, cache detail, microarchitecture detail and vulnerability mitigations.

#[allow(unused_imports)]
use super::super::resolve::*;
#[allow(unused_imports)]
use super::super::{Domain as D, Entity, EntityKind as K, Provenance as P, Unit as U};

pub(super) fn declare(_add: &mut dyn FnMut(Entity)) {}

pub(super) fn resolve(_out: &mut Vec<Reading>) {}
