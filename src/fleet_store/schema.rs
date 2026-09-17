//! The table schema, and the one rule it exists to preserve.
//!
//! **Every measurement column is nullable, and that is not laziness.** This
//! crate's whole discipline is that a reading which was not taken is not a
//! reading of zero: `Snapshot` carries `Option` on every metric, the CSV export
//! emits an empty field rather than `0`, and the `tsdb` module's header explains
//! that a `0` in a version-5 file "may be a measurement of an idle machine or a
//! tick whose read failed, and nothing in the file says which".
//!
//! A columnar store is where that distinction is most easily lost, because
//! zero-filling a column is the path of least resistance and the resulting table
//! looks perfectly healthy. An idle GPU and an absent GPU would then average
//! together, and a fleet-wide "mean GPU utilisation" would silently include
//! machines that have no GPU at all. So the required columns here are only the
//! ones that identify a row — the host and the instant — and everything that was
//! *read* may be null.
//!
//! The corollary for anyone querying it: `AVG(gpu_utilization)` over this table
//! already does the right thing, because SQL aggregates skip nulls. That is the
//! point of paying for nullability.

use iceberg::spec::{NestedField, PrimitiveType, Schema, Type};

/// Field ids are part of the table's identity in Iceberg, not an implementation
/// detail: renaming a column keeps its id, and a reader resolves by id rather
/// than by name. They are assigned explicitly and must never be reused for a
/// different meaning.
mod field_id {
    pub const HOST_ID: i32 = 1;
    pub const COLLECTED_AT: i32 = 2;
    pub const GENERATION: i32 = 3;

    pub const CPU_UTILIZATION: i32 = 10;
    pub const CPU_CORES: i32 = 11;
    // 12 was `cpu_frequency_mhz`, removed before it ever shipped: the per-core
    // frequencies would have had to be aggregated into one number, and every
    // available aggregation (first core, mean, max) asserts something the
    // reading does not say. The id is retired rather than reused — a reader
    // resolves by id, so giving 12 a new meaning would silently reinterpret any
    // table that ever carried the old one.

    pub const MEMORY_USED_BYTES: i32 = 20;
    pub const MEMORY_TOTAL_BYTES: i32 = 21;
    pub const SWAP_USED_BYTES: i32 = 22;
    pub const SWAP_TOTAL_BYTES: i32 = 23;

    pub const GPU_INDEX: i32 = 30;
    pub const GPU_NAME: i32 = 31;
    pub const GPU_UTILIZATION: i32 = 32;
    pub const GPU_MEMORY_USED_BYTES: i32 = 33;
    pub const GPU_MEMORY_TOTAL_BYTES: i32 = 34;
    pub const GPU_TEMPERATURE_C: i32 = 35;
    pub const GPU_POWER_MW: i32 = 36;

    pub const NET_RX_BYTES_PER_SEC: i32 = 40;
    pub const NET_TX_BYTES_PER_SEC: i32 = 41;

    pub const COLLECT_US: i32 = 50;
}

/// The name of the per-host, per-tick metrics table.
pub const TABLE_NAME: &str = "host_metrics";

/// One row per host per collector tick, with GPUs flattened.
///
/// **GPUs are flattened rather than nested**, so a three-card machine emits
/// three rows for one tick and a card-less machine emits one row with the GPU
/// columns null. A list-of-struct column would model the tick more faithfully,
/// but every engine that would query this — DataFusion, Spark, Trino, DuckDB —
/// reaches a nested column through an explode, and the queries this table exists
/// to answer are per-card: which card in which host is hot, which is idle.
/// `(host_id, collected_at, gpu_index)` identifies a card's sample.
///
/// The cost is that the host-level columns repeat per GPU row, so a naive
/// `SUM(memory_used_bytes)` over a multi-GPU host double counts. Parquet's
/// run-length encoding makes the storage cost of the repetition close to
/// nothing; the query hazard is real and is the reason this is documented here
/// rather than only in a commit message.
pub fn host_metrics_schema() -> Result<Schema, iceberg::Error> {
    Schema::builder()
        .with_schema_id(0)
        // Identity: the only two columns a row cannot be without. A row that
        // does not say which machine or which instant is not a measurement of
        // anything.
        .with_identifier_field_ids(vec![field_id::HOST_ID, field_id::COLLECTED_AT])
        .with_fields(vec![
            NestedField::required(
                field_id::HOST_ID,
                "host_id",
                Type::Primitive(PrimitiveType::String),
            )
            .into(),
            NestedField::required(
                field_id::COLLECTED_AT,
                "collected_at",
                Type::Primitive(PrimitiveType::Timestamptz),
            )
            .into(),
            // The collector's monotonic tick counter. Required because it is
            // produced by the collector rather than read from hardware, so it
            // cannot fail to be available the way a sensor can.
            NestedField::required(
                field_id::GENERATION,
                "generation",
                Type::Primitive(PrimitiveType::Long),
            )
            .into(),
            // Everything below here was read from the machine, so everything
            // below here is nullable. See the module header.
            NestedField::optional(
                field_id::CPU_UTILIZATION,
                "cpu_utilization",
                Type::Primitive(PrimitiveType::Double),
            )
            .into(),
            NestedField::optional(
                field_id::CPU_CORES,
                "cpu_cores",
                Type::Primitive(PrimitiveType::Int),
            )
            .into(),
            NestedField::optional(
                field_id::MEMORY_USED_BYTES,
                "memory_used_bytes",
                Type::Primitive(PrimitiveType::Long),
            )
            .into(),
            NestedField::optional(
                field_id::MEMORY_TOTAL_BYTES,
                "memory_total_bytes",
                Type::Primitive(PrimitiveType::Long),
            )
            .into(),
            NestedField::optional(
                field_id::SWAP_USED_BYTES,
                "swap_used_bytes",
                Type::Primitive(PrimitiveType::Long),
            )
            .into(),
            NestedField::optional(
                field_id::SWAP_TOTAL_BYTES,
                "swap_total_bytes",
                Type::Primitive(PrimitiveType::Long),
            )
            .into(),
            // Null on a machine with no accelerators, which is a different fact
            // from a card reporting 0%.
            NestedField::optional(
                field_id::GPU_INDEX,
                "gpu_index",
                Type::Primitive(PrimitiveType::Int),
            )
            .into(),
            NestedField::optional(
                field_id::GPU_NAME,
                "gpu_name",
                Type::Primitive(PrimitiveType::String),
            )
            .into(),
            NestedField::optional(
                field_id::GPU_UTILIZATION,
                "gpu_utilization",
                Type::Primitive(PrimitiveType::Double),
            )
            .into(),
            NestedField::optional(
                field_id::GPU_MEMORY_USED_BYTES,
                "gpu_memory_used_bytes",
                Type::Primitive(PrimitiveType::Long),
            )
            .into(),
            NestedField::optional(
                field_id::GPU_MEMORY_TOTAL_BYTES,
                "gpu_memory_total_bytes",
                Type::Primitive(PrimitiveType::Long),
            )
            .into(),
            NestedField::optional(
                field_id::GPU_TEMPERATURE_C,
                "gpu_temperature_c",
                Type::Primitive(PrimitiveType::Double),
            )
            .into(),
            NestedField::optional(
                field_id::GPU_POWER_MW,
                "gpu_power_mw",
                Type::Primitive(PrimitiveType::Long),
            )
            .into(),
            NestedField::optional(
                field_id::NET_RX_BYTES_PER_SEC,
                "net_rx_bytes_per_sec",
                Type::Primitive(PrimitiveType::Double),
            )
            .into(),
            NestedField::optional(
                field_id::NET_TX_BYTES_PER_SEC,
                "net_tx_bytes_per_sec",
                Type::Primitive(PrimitiveType::Double),
            )
            .into(),
            // What the tick cost to gather. Useful for exactly the question a
            // fleet operator asks when the numbers look wrong: was the collector
            // keeping up on this host?
            NestedField::optional(
                field_id::COLLECT_US,
                "collect_us",
                Type::Primitive(PrimitiveType::Long),
            )
            .into(),
        ])
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The schema must build at all, and must keep its identity columns.
    #[test]
    fn the_schema_builds_and_identifies_a_row_by_host_and_instant() {
        let schema = host_metrics_schema().expect("schema must build");

        let host = schema
            .field_by_name("host_id")
            .expect("host_id must be present");
        assert!(host.required, "a row must say which machine it came from");

        let at = schema
            .field_by_name("collected_at")
            .expect("collected_at must be present");
        assert!(at.required, "a row must say when it was taken");
    }

    /// **The rule this module exists for.** Every column holding a reading must
    /// be nullable, so that "not measured" survives into the table instead of
    /// arriving as a zero that averages like an idle machine.
    #[test]
    fn every_measured_column_is_nullable() {
        let schema = host_metrics_schema().expect("schema must build");

        // The only required columns are the ones the collector produces itself
        // rather than reads from hardware.
        const PRODUCED_NOT_READ: [&str; 3] = ["host_id", "collected_at", "generation"];

        let mut wrongly_required = Vec::new();
        for field in schema.as_struct().fields() {
            if PRODUCED_NOT_READ.contains(&field.name.as_str()) {
                continue;
            }
            if field.required {
                wrongly_required.push(field.name.clone());
            }
        }

        assert!(
            wrongly_required.is_empty(),
            "these columns hold readings and must be nullable, or a failed read \
             becomes a zero and averages like an idle machine: {wrongly_required:?}"
        );
    }

    /// Field ids are the table's identity across renames, so a duplicate is a
    /// corruption rather than a style problem.
    #[test]
    fn no_field_id_is_used_twice() {
        let schema = host_metrics_schema().expect("schema must build");
        let mut seen: Vec<i32> = schema.as_struct().fields().iter().map(|f| f.id).collect();
        let before = seen.len();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(
            before,
            seen.len(),
            "two columns share a field id; a reader resolves by id, so they would alias"
        );
    }

    /// A GPU-less host must still be representable, or the table cannot hold
    /// half a fleet.
    #[test]
    fn the_gpu_columns_are_optional_so_a_card_less_host_still_has_rows() {
        let schema = host_metrics_schema().expect("schema must build");
        for name in [
            "gpu_index",
            "gpu_name",
            "gpu_utilization",
            "gpu_memory_used_bytes",
            "gpu_temperature_c",
        ] {
            let f = schema
                .field_by_name(name)
                .unwrap_or_else(|| panic!("{name} must exist"));
            assert!(
                !f.required,
                "{name} must be optional: a machine with no accelerators still reports"
            );
        }
    }
}
