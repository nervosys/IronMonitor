//! Writing snapshots into the table.
//!
//! The append path is Iceberg's standard composition — a Parquet file writer
//! inside a rolling file writer inside a data file writer — followed by a
//! `fast_append` transaction. Nothing clever, and the comments here are about
//! the two places where the obvious thing would be wrong.
//!
//! **Nothing is created implicitly.** [`FleetStore::open`] loads a table and
//! fails if it is not there. Creating it is a separate, explicit call, because a
//! typo in a table name that silently creates a second table is a fleet split in
//! half with nothing reporting an error.
//!
//! **A failed append is reported, never swallowed.** This runs off the
//! collector's publish hook, where the temptation is to log and continue so the
//! UI keeps moving. That is right for the *process* and wrong for the *caller*:
//! a store that quietly drops ticks under load produces a table with holes in it
//! that look exactly like an idle machine — the same confusion between "no data"
//! and "zero" that the schema exists to prevent, arriving through the back door.
//! So every append returns its outcome, and the decision to continue belongs to
//! whoever wired it up.

use std::collections::HashMap;
use std::sync::Arc;

use iceberg::spec::DataFileFormat;
use iceberg::table::Table;
use iceberg::transaction::{ApplyTransactionAction, Transaction};
use iceberg::writer::base_writer::data_file_writer::DataFileWriterBuilder;
use iceberg::writer::file_writer::location_generator::{
    DefaultFileNameGenerator, DefaultLocationGenerator,
};
use iceberg::writer::file_writer::rolling_writer::RollingFileWriterBuilder;
use iceberg::writer::file_writer::ParquetWriterBuilder;
use iceberg::writer::{IcebergWriter, IcebergWriterBuilder};
use iceberg::{Catalog, NamespaceIdent, TableCreation, TableIdent};
use parquet::file::properties::WriterProperties;

use crate::pipeline::Snapshot;

use super::{rows_from_snapshot, rows_to_record_batch, HostId};

/// A handle to one fleet metrics table.
pub struct FleetStore {
    catalog: Arc<dyn Catalog>,
    ident: TableIdent,
    host: HostId,
    /// Makes each append's data file name unique. See [`Self::file_name_token`].
    appends: std::sync::atomic::AtomicU64,
    /// Fixed when this store is opened, so two runs of the same host on the
    /// same table do not regenerate the same names.
    opened_at_nanos: u128,
}

impl FleetStore {
    /// Open an existing table.
    ///
    /// Fails if the table does not exist. See the module header for why this
    /// does not create one.
    pub async fn open(
        catalog: Arc<dyn Catalog>,
        ident: TableIdent,
        host: HostId,
    ) -> Result<Self, iceberg::Error> {
        // Loaded here purely to fail early: a store that only discovers the
        // table is missing on the first append has already lost a tick.
        let _ = catalog.load_table(&ident).await?;
        Ok(Self {
            catalog,
            ident,
            host,
            appends: std::sync::atomic::AtomicU64::new(0),
            opened_at_nanos: now_nanos(),
        })
    }

    /// Create the table, then open it.
    ///
    /// Separate from [`Self::open`] and explicit at the call site. `location` is
    /// the operator's warehouse path; there is no default, because a default
    /// here would be this crate choosing where a fleet's data lives.
    pub async fn create(
        catalog: Arc<dyn Catalog>,
        namespace: NamespaceIdent,
        table_name: &str,
        location: &str,
        host: HostId,
    ) -> Result<Self, iceberg::Error> {
        if catalog.get_namespace(&namespace).await.is_err() {
            catalog.create_namespace(&namespace, HashMap::new()).await?;
        }

        let creation = TableCreation::builder()
            .name(table_name.to_owned())
            .location(location.to_owned())
            .schema(super::host_metrics_schema()?)
            .build();

        let table = catalog.create_table(&namespace, creation).await?;
        Ok(Self {
            catalog,
            ident: table.identifier().clone(),
            host,
            appends: std::sync::atomic::AtomicU64::new(0),
            opened_at_nanos: now_nanos(),
        })
    }

    /// The host identifier every row this store writes will carry.
    pub fn host(&self) -> &HostId {
        &self.host
    }

    /// A token making this append's data file name unique.
    ///
    /// **`DefaultFileNameGenerator` counts from zero for every writer it is
    /// given to**, and this code builds a fresh writer per append — so without a
    /// token every tick produces `ironmon-00000.parquet` and the second commit
    /// fails with *"Cannot add files that are already referenced by table"*. A
    /// failure is the good outcome there; the bad one is a store that overwrites
    /// yesterday's data and reports success.
    ///
    /// Three parts, each covering a case the others do not: the host, so two
    /// machines writing to one table cannot collide; the time this store was
    /// opened, so a restarted process does not resume at the same names; and a
    /// counter, so appends within one run stay distinct however fast they come.
    fn file_name_token(&self) -> String {
        let n = self
            .appends
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        // Eight characters of the host id is 32 bits, which is plenty to
        // separate hosts within one table when the other two parts are present.
        let host = &self.host.as_str()[..8.min(self.host.as_str().len())];
        format!("{host}-{:x}-{n:x}", self.opened_at_nanos)
    }

    /// Append one snapshot's rows, returning how many rows were written.
    ///
    /// **Returns `Ok(0)` and commits nothing for a warm-up snapshot.** The
    /// pipeline publishes its first generation from an empty `Sources`, so it
    /// describes no GPUs, no processes and no connections *by construction*.
    /// Storing it would put a row in the table asserting that a three-card
    /// machine had no accelerators at that instant — a fabricated reading, in
    /// permanent storage, indistinguishable from a real one later. This is the
    /// same warm-up that made `--frame` print `GPU:0` and that fitted a window
    /// to a GPU-less Overview; here it would be worse, because a screen is
    /// redrawn a second later and a table is not.
    pub async fn append(&self, snapshot: &Snapshot) -> Result<usize, iceberg::Error> {
        if snapshot.warmup {
            return Ok(0);
        }

        let rows = rows_from_snapshot(snapshot, &self.host);
        if rows.is_empty() {
            return Ok(0);
        }

        let table = self.catalog.load_table(&self.ident).await?;
        // The table's schema, not this crate's: Iceberg reassigns field ids at
        // creation, so the two agree on names and order but not on ids, and the
        // Parquet writer resolves by id.
        let arrow_schema =
            iceberg::arrow::schema_to_arrow_schema(table.metadata().current_schema())?;
        let batch = rows_to_record_batch(&rows, &arrow_schema)?;
        let data_files = write_batch(&table, batch, &self.file_name_token()).await?;
        if data_files.is_empty() {
            return Ok(0);
        }

        let tx = Transaction::new(&table);
        let action = tx.fast_append().add_data_files(data_files);
        let tx = action.apply(tx)?;
        tx.commit(self.catalog.as_ref()).await?;

        Ok(rows.len())
    }
}

/// Write one batch to Parquet and return the resulting data files.
async fn write_batch(
    table: &Table,
    batch: arrow_array::RecordBatch,
    token: &str,
) -> Result<Vec<iceberg::spec::DataFile>, iceberg::Error> {
    let location_generator = DefaultLocationGenerator::new(table.metadata())?;
    let file_name_generator = DefaultFileNameGenerator::new(
        "ironmon".to_owned(),
        // Required, not decorative: see `FleetStore::file_name_token`.
        Some(token.to_owned()),
        DataFileFormat::Parquet,
    );

    let parquet_writer_builder = ParquetWriterBuilder::new(
        WriterProperties::default(),
        table.metadata().current_schema().clone(),
    );
    let rolling = RollingFileWriterBuilder::new_with_default_file_size(
        parquet_writer_builder,
        table.file_io().clone(),
        location_generator,
        file_name_generator,
    );

    let mut writer = DataFileWriterBuilder::new(rolling).build(None).await?;
    writer.write(batch).await?;
    writer.close().await
}

fn now_nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

/// Shared setup for tests in this module and in [`super::sink`].
#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use iceberg::memory::{MemoryCatalogBuilder, MEMORY_CATALOG_WAREHOUSE};
    use iceberg::CatalogBuilder;

    /// A warehouse directory the test owns and removes on drop.
    ///
    /// Returned alongside the store rather than dropped immediately: the files
    /// the store writes live under it, and a test that lets it fall out of
    /// scope deletes the table it is about to assert on.
    pub struct Warehouse {
        path: std::path::PathBuf,
    }

    impl Warehouse {
        pub fn new(tag: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "ironmon-fleet-{tag}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("warehouse dir");
            Self { path }
        }

        /// A `file://` URL, which is what the catalog and the file IO expect.
        pub fn url(&self) -> String {
            format!(
                "file:///{}",
                self.path.display().to_string().replace('\\', "/")
            )
        }
    }

    impl Drop for Warehouse {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    pub async fn catalog_for(warehouse: &Warehouse) -> impl Catalog {
        MemoryCatalogBuilder::default()
            .load(
                "memory",
                HashMap::from([(MEMORY_CATALOG_WAREHOUSE.to_string(), warehouse.url())]),
            )
            .await
            .expect("catalog")
    }

    /// A created, empty table in a fresh warehouse.
    pub async fn in_memory_store(tag: &str) -> (Warehouse, FleetStore) {
        let warehouse = Warehouse::new(tag);
        let catalog = catalog_for(&warehouse).await;
        let store = FleetStore::create(
            Arc::new(catalog),
            NamespaceIdent::new("ironmon".to_owned()),
            super::super::TABLE_NAME,
            &format!("{}/{tag}", warehouse.url()),
            HostId::rotate(Some("test-host")),
        )
        .await
        .expect("create table");
        (warehouse, store)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::Snapshot;

    use test_support::in_memory_store;

    /// The whole path, end to end: a snapshot becomes rows in a real table.
    #[tokio::test]
    async fn a_snapshot_becomes_rows_in_the_table() {
        let (_warehouse, store) = in_memory_store("append").await;

        let snapshot = Snapshot {
            generation: 3,
            collected_at: 1_700_000_000,
            ..Default::default()
        };

        let written = store.append(&snapshot).await.expect("append");
        assert_eq!(written, 1, "a GPU-less host contributes one row");
    }

    /// **The warm-up must never reach permanent storage.**
    ///
    /// It is built from an empty source set, so it describes a machine with no
    /// accelerators whatever the machine actually has. On a screen that is a
    /// frame that gets redrawn; in a table it is a fabricated reading that
    /// outlives the process.
    #[tokio::test]
    async fn a_warm_up_snapshot_is_not_stored() {
        let (_warehouse, store) = in_memory_store("warmup").await;

        let warmup = Snapshot {
            generation: 0,
            warmup: true,
            ..Default::default()
        };

        let written = store.append(&warmup).await.expect("append");
        assert_eq!(
            written, 0,
            "the warm-up generation describes no GPUs by construction and must not be stored"
        );
    }

    /// Opening a table that does not exist must fail rather than create one.
    #[tokio::test]
    async fn opening_a_missing_table_is_an_error() {
        let warehouse = test_support::Warehouse::new("missing");
        let catalog = test_support::catalog_for(&warehouse).await;

        let result = FleetStore::open(
            Arc::new(catalog),
            TableIdent::from_strs(["ironmon", "not_here"]).expect("ident"),
            HostId::rotate(Some("h")),
        )
        .await;

        assert!(
            result.is_err(),
            "a missing table must be reported, not created: a typo would otherwise \
             split a fleet across two tables with nothing saying so"
        );
    }

    /// Read every row back out of the table.
    ///
    /// The tests below assert on stored data rather than on what `append`
    /// returned, because a write path can report success and still have put the
    /// wrong thing on disk — which is precisely the failure this schema exists
    /// to prevent.
    async fn read_back(store: &FleetStore) -> Vec<arrow_array::RecordBatch> {
        use futures::TryStreamExt;

        let table = store.catalog.load_table(&store.ident).await.expect("load");
        let scan = table.scan().build().expect("scan");
        scan.to_arrow()
            .await
            .expect("arrow stream")
            .try_collect::<Vec<_>>()
            .await
            .expect("collect")
    }

    /// **The round trip that matters.** An unread metric must still be null
    /// after Parquet, and a measured zero must still be zero.
    ///
    /// Everything else in this module checks that a write was accepted. This
    /// checks what came back, which is the only version of the guarantee a
    /// query will ever see.
    #[tokio::test]
    async fn a_null_survives_the_round_trip_and_a_zero_stays_a_zero() {
        use arrow_array::{Array, Float64Array};

        let (_warehouse, store) = in_memory_store("roundtrip").await;

        // One card reporting 0% and one reporting nothing, in the same tick.
        let snapshot = Snapshot {
            generation: 1,
            collected_at: 1_700_000_000,
            gpu_static: vec![
                super::super::rows::fixtures::card(0, "idle"),
                super::super::rows::fixtures::card(1, "unread"),
            ],
            gpu_dynamic: vec![
                Some(super::super::rows::fixtures::reading(Some(0), Some(40))),
                Some(super::super::rows::fixtures::reading(None, None)),
            ],
            ..Default::default()
        };
        assert_eq!(store.append(&snapshot).await.expect("append"), 2);

        let batches = read_back(&store).await;
        let total: usize = batches.iter().map(|b| b.num_rows()).sum();
        assert_eq!(total, 2, "both cards must be on disk");

        let mut seen_zero = false;
        let mut seen_null = false;
        for batch in &batches {
            let col = batch
                .column_by_name("gpu_utilization")
                .expect("gpu_utilization column");
            let values = col
                .as_any()
                .downcast_ref::<Float64Array>()
                .expect("f64 column");
            for row in 0..batch.num_rows() {
                if values.is_null(row) {
                    seen_null = true;
                } else if values.value(row) == 0.0 {
                    seen_zero = true;
                }
            }
        }

        assert!(
            seen_zero,
            "the card that reported 0% must read back as 0, not as null"
        );
        assert!(
            seen_null,
            "the card that reported nothing must read back as null, not as 0 —              a zero here averages like an idle card forever after"
        );
    }

    /// A host with no accelerators must be readable as such, rather than
    /// absent from the table.
    #[tokio::test]
    async fn a_gpu_less_host_reads_back_with_null_gpu_columns() {
        use arrow_array::Array;

        let (_warehouse, store) = in_memory_store("gpuless-rt").await;
        let snapshot = Snapshot {
            generation: 1,
            collected_at: 1_700_000_000,
            ..Default::default()
        };
        assert_eq!(store.append(&snapshot).await.expect("append"), 1);

        let batches = read_back(&store).await;
        let batch = batches.first().expect("one batch");
        assert_eq!(batch.num_rows(), 1);

        for name in ["gpu_index", "gpu_name", "gpu_utilization"] {
            let col = batch
                .column_by_name(name)
                .unwrap_or_else(|| panic!("{name} column"));
            assert_eq!(
                col.null_count(),
                1,
                "{name} must be null for a host with no accelerators"
            );
        }

        // And the host-level readings it did take are present.
        let host = batch.column_by_name("host_id").expect("host_id");
        assert_eq!(host.null_count(), 0, "every row must name its host");
    }

    /// Several ticks accumulate rather than replacing one another.
    #[tokio::test]
    async fn successive_ticks_accumulate() {
        let (_warehouse, store) = in_memory_store("accumulate").await;

        let mut total = 0;
        for generation in 1..=3 {
            let snapshot = Snapshot {
                generation,
                collected_at: 1_700_000_000 + generation,
                ..Default::default()
            };
            total += store.append(&snapshot).await.expect("append");
        }

        assert_eq!(total, 3, "each tick must add its own rows");
    }
}
