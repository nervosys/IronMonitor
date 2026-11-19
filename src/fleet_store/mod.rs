//! Fleet-scale metric storage on Apache Iceberg.
//!
//! # This is storage the operator runs, not telemetry this crate sends
//!
//! Read this before anything else here, because the distinction decides whether
//! the module is allowed to exist at all.
//!
//! [`crate::consent`] states, in its module header and on the `ironmon
//! privacy` screens, exactly what IronMonitor sends to its authors — system
//! telemetry from `ironmon serve`, opt-out — and it is guarded:
//! `granting_an_unimplemented_scope_still_collects_nothing` grants every other
//! consent scope and then asserts that each still reports it collects
//! nothing. That test exists to force the documentation and the CLI screens to
//! be rewritten in the same change as any real collector.
//!
//! **This module does not trip it, and the reason is the direction of the
//! data.** What `consent` governs is IronMonitor gathering facts about a user's
//! machine and sending them somewhere the *vendor* chose — telemetry.nervosys.ai,
//! under its own scope. What this module does is write a machine's own metrics
//! to a table the *operator* configured and points at, which is the same
//! category as [`crate::tsdb`] writing a local time-series file and
//! [`crate::prometheus`] exposing metrics for the operator's own scraper.
//!
//! Three properties keep the distinction honest, and each is enforced rather
//! than promised:
//!
//! - **Off unless compiled in.** The `fleet-store` feature is not in `full` and
//!   not in `default`. A stock build has no Iceberg in its dependency graph.
//! - **Off unless configured.** There is no default catalog, no default
//!   endpoint and no fallback location. Nothing is written until an operator
//!   names a table.
//! - **No hardware identifiers.** The join key is a locally generated random
//!   value, not a board UUID or a MAC. See [`host_id`], which exists entirely
//!   for this reason.
//!
//! # Why Iceberg rather than the existing store
//!
//! [`crate::tsdb`] is a single-machine append-only file with a bincode row
//! format and size-triggered rotation. It is a good fit for what it does and a
//! poor one for a fleet: there is no host column in its rows at all, so every
//! file is implicitly one machine, and answering "which card in which host ran
//! hottest last week" means opening every file in sequence and decoding every
//! record to reach four columns.
//!
//! Iceberg gives the three things that gap needs — a columnar layout so a query
//! touching four columns reads four columns, snapshot isolation so appends from
//! many hosts do not race, and a catalog so the table is discoverable by the
//! engines an operator already runs. It costs a ~50-crate dependency graph and,
//! more sharply, **a Rust floor of 1.94 against this crate's previous 1.89**;
//! `Cargo.toml` records that trade in full.

pub mod arrow;
pub mod host_id;
pub mod rows;
pub mod schema;
pub mod sink;
pub mod store;

pub use arrow::{arrow_schema, rows_to_record_batch};
pub use host_id::HostId;
pub use rows::{rows_from_snapshot, MetricRow};
pub use schema::{host_metrics_schema, TABLE_NAME};
pub use sink::{FleetSink, SinkStats};
pub use store::FleetStore;
