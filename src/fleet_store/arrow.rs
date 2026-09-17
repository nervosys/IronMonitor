//! Turning [`MetricRow`]s into an Arrow `RecordBatch`.
//!
//! Mechanical, with one thing worth guarding: **a null must stay a null.** Every
//! Arrow builder in this module is fed an `Option`, never an unwrapped value
//! with a default, because `unwrap_or(0.0)` is how the distinction the rest of
//! this crate protects would finally be lost — one line, in the least
//! interesting file, converting "not measured" into "measured zero" on its way
//! into permanent storage.
//!
//! The Arrow schema is derived from the Iceberg schema rather than written out
//! again here. Two hand-maintained column lists drift, and the failure when they
//! do is a column silently holding another column's values — both are `i64`, so
//! nothing complains.

use std::sync::Arc;

use arrow_array::{
    ArrayRef, Float64Array, Int32Array, Int64Array, RecordBatch, StringArray,
    TimestampMicrosecondArray,
};
use arrow_schema::{DataType, Schema as ArrowSchema};

use super::rows::MetricRow;

/// The Arrow schema for `host_metrics`, derived from the Iceberg schema.
///
/// Derived rather than declared, so the two cannot disagree. This also carries
/// the Iceberg field-id metadata that the Parquet writer needs in order to
/// produce files a reader can resolve by id.
pub fn arrow_schema() -> Result<ArrowSchema, iceberg::Error> {
    let iceberg_schema = super::schema::host_metrics_schema()?;
    iceberg::arrow::schema_to_arrow_schema(&iceberg_schema)
}

/// Build one `RecordBatch` from a batch of rows, against a given Arrow schema.
///
/// **The schema is a parameter, and it must be the table's.** Iceberg reassigns
/// field ids when a table is created — `TableMetadataBuilder::reassign_ids`
/// renumbers them densely from 1 — so the ids in [`super::schema`] describe the
/// *request*, not the table that results. Building a batch against the local
/// schema and writing it to a created table fails with `Field id 4 not found in
/// struct array`, which names an id this crate never assigns and does not
/// mention schemas at all.
///
/// An empty slice produces an empty batch rather than an error: a tick that
/// contributed no rows is not a failure, and callers should not have to special
/// case it.
pub fn rows_to_record_batch(
    rows: &[MetricRow],
    schema: &ArrowSchema,
) -> Result<RecordBatch, iceberg::Error> {
    let schema = Arc::new(schema.clone());

    // The timezone is taken from the schema rather than assumed, because
    // `RecordBatch::try_new` compares data types exactly and a mismatched
    // timezone string fails with a message about `Timestamp` that does not
    // mention timezones at all.
    let timestamp_tz =
        schema
            .field_with_name("collected_at")
            .ok()
            .and_then(|f| match f.data_type() {
                DataType::Timestamp(_, tz) => tz.clone(),
                _ => None,
            });

    let mut collected_at =
        TimestampMicrosecondArray::from(rows.iter().map(|r| r.collected_at_us).collect::<Vec<_>>());
    if let Some(tz) = timestamp_tz {
        collected_at = collected_at.with_timezone(tz);
    }

    // Every column below takes `Option` straight through. See the module header.
    let columns: Vec<ArrayRef> = vec![
        Arc::new(StringArray::from(
            rows.iter().map(|r| r.host_id.as_str()).collect::<Vec<_>>(),
        )),
        Arc::new(collected_at),
        Arc::new(Int64Array::from(
            rows.iter().map(|r| r.generation).collect::<Vec<_>>(),
        )),
        Arc::new(Float64Array::from(
            rows.iter().map(|r| r.cpu_utilization).collect::<Vec<_>>(),
        )),
        Arc::new(Int32Array::from(
            rows.iter().map(|r| r.cpu_cores).collect::<Vec<_>>(),
        )),
        Arc::new(Int64Array::from(
            rows.iter().map(|r| r.memory_used_bytes).collect::<Vec<_>>(),
        )),
        Arc::new(Int64Array::from(
            rows.iter()
                .map(|r| r.memory_total_bytes)
                .collect::<Vec<_>>(),
        )),
        Arc::new(Int64Array::from(
            rows.iter().map(|r| r.swap_used_bytes).collect::<Vec<_>>(),
        )),
        Arc::new(Int64Array::from(
            rows.iter().map(|r| r.swap_total_bytes).collect::<Vec<_>>(),
        )),
        Arc::new(Int32Array::from(
            rows.iter().map(|r| r.gpu_index).collect::<Vec<_>>(),
        )),
        Arc::new(StringArray::from(
            rows.iter()
                .map(|r| r.gpu_name.as_deref())
                .collect::<Vec<_>>(),
        )),
        Arc::new(Float64Array::from(
            rows.iter().map(|r| r.gpu_utilization).collect::<Vec<_>>(),
        )),
        Arc::new(Int64Array::from(
            rows.iter()
                .map(|r| r.gpu_memory_used_bytes)
                .collect::<Vec<_>>(),
        )),
        Arc::new(Int64Array::from(
            rows.iter()
                .map(|r| r.gpu_memory_total_bytes)
                .collect::<Vec<_>>(),
        )),
        Arc::new(Float64Array::from(
            rows.iter().map(|r| r.gpu_temperature_c).collect::<Vec<_>>(),
        )),
        Arc::new(Int64Array::from(
            rows.iter().map(|r| r.gpu_power_mw).collect::<Vec<_>>(),
        )),
        Arc::new(Float64Array::from(
            rows.iter()
                .map(|r| r.net_rx_bytes_per_sec)
                .collect::<Vec<_>>(),
        )),
        Arc::new(Float64Array::from(
            rows.iter()
                .map(|r| r.net_tx_bytes_per_sec)
                .collect::<Vec<_>>(),
        )),
        Arc::new(Int64Array::from(
            rows.iter().map(|r| r.collect_us).collect::<Vec<_>>(),
        )),
    ];

    RecordBatch::try_new(schema, columns).map_err(|e| {
        iceberg::Error::new(
            iceberg::ErrorKind::Unexpected,
            format!("host_metrics rows do not match the table schema: {e}"),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fleet_store::HostId;
    use arrow_array::Array;

    fn row(gpu_utilization: Option<f64>, gpu_name: Option<&str>) -> MetricRow {
        MetricRow {
            host_id: HostId::rotate(Some("h")).as_str().to_owned(),
            collected_at_us: 1_700_000_000_000_000,
            generation: 1,
            cpu_utilization: Some(12.5),
            cpu_cores: Some(24),
            memory_used_bytes: Some(1024),
            memory_total_bytes: Some(2048),
            swap_used_bytes: None,
            swap_total_bytes: None,
            gpu_index: gpu_name.map(|_| 0),
            gpu_name: gpu_name.map(str::to_owned),
            gpu_utilization,
            gpu_memory_used_bytes: None,
            gpu_memory_total_bytes: None,
            gpu_temperature_c: None,
            gpu_power_mw: None,
            net_rx_bytes_per_sec: None,
            net_tx_bytes_per_sec: None,
            collect_us: Some(4_200),
        }
    }

    /// Build against the crate's own schema, which is what an unattached test
    /// has. A real append uses the table's; see `rows_to_record_batch`.
    fn batch(rows: &[MetricRow]) -> RecordBatch {
        let schema = arrow_schema().expect("schema");
        rows_to_record_batch(rows, &schema).expect("batch")
    }

    /// The column count and order must match the schema, or columns alias.
    #[test]
    fn the_batch_matches_the_table_schema() {
        let batch = batch(&[row(Some(1.0), Some("card"))]);
        let schema = arrow_schema().expect("schema");
        assert_eq!(batch.num_columns(), schema.fields().len());
        assert_eq!(batch.num_rows(), 1);
    }

    /// **The guard this module exists for.** An unread metric must arrive in
    /// Arrow as a null, not as a zero.
    #[test]
    fn an_unread_metric_is_null_in_the_batch_rather_than_zero() {
        let batch = batch(&[row(None, Some("card"))]);

        let column = batch
            .column_by_name("gpu_utilization")
            .expect("gpu_utilization column");
        assert_eq!(
            column.null_count(),
            1,
            "an unread utilisation must be null; a zero here averages like an idle card"
        );
        assert!(column.is_null(0));
    }

    /// A measured zero must survive as a zero, which is the other half of the
    /// same rule and the easier one to break while fixing the first.
    #[test]
    fn a_measured_zero_is_not_turned_into_a_null() {
        let batch = batch(&[row(Some(0.0), Some("card"))]);

        let column = batch
            .column_by_name("gpu_utilization")
            .expect("gpu_utilization column");
        assert_eq!(column.null_count(), 0, "0% is a reading and must be stored");

        let values = column
            .as_any()
            .downcast_ref::<Float64Array>()
            .expect("f64 column");
        assert_eq!(values.value(0), 0.0);
    }

    /// A GPU-less host writes a row whose GPU columns are all null.
    #[test]
    fn a_gpu_less_row_nulls_every_gpu_column() {
        let batch = batch(&[row(None, None)]);
        for name in ["gpu_index", "gpu_name", "gpu_utilization", "gpu_power_mw"] {
            let column = batch
                .column_by_name(name)
                .unwrap_or_else(|| panic!("{name} column"));
            assert_eq!(
                column.null_count(),
                1,
                "{name} must be null on a card-less host"
            );
        }
    }

    /// A tick that produced no rows is not an error.
    #[test]
    fn an_empty_batch_is_allowed() {
        let batch = batch(&[]);
        assert_eq!(batch.num_rows(), 0);
    }

    /// Mixed rows must keep each row's own nullness rather than one row's shape
    /// deciding the column.
    #[test]
    fn nulls_are_per_row_not_per_column() {
        let batch = batch(&[
            row(Some(50.0), Some("read")),
            row(None, Some("unread")),
            row(Some(0.0), Some("idle")),
        ]);

        let column = batch
            .column_by_name("gpu_utilization")
            .expect("gpu_utilization column");
        assert_eq!(column.null_count(), 1);
        assert!(!column.is_null(0));
        assert!(column.is_null(1));
        assert!(!column.is_null(2));
    }
}
