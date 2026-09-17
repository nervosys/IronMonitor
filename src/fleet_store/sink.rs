//! Driving the store from the collector.
//!
//! # The publish hook is a doorbell, not a delivery
//!
//! [`crate::pipeline::PublishHook`] is `Arc<dyn Fn() + Send + Sync>`. It takes
//! **no snapshot**, and its documentation says it "must not block: it runs
//! inline on the collector between the store and the next tick's sleep."
//!
//! An Iceberg append writes a Parquet file and commits to a catalog. Performing
//! that inline would stall collection every tick, on the thread whose timing the
//! whole pipeline's cadence depends on — a store that made the monitor worse at
//! monitoring. So the hook only rings a bell, and the append happens in a task
//! that reads [`crate::pipeline::SnapshotHandle::latest`] when woken. The hook's
//! signature was already saying this: it passes no data because it was never
//! meant to deliver any.
//!
//! # Falling behind is normal, and must be visible
//!
//! Appending is slower than collecting and will sometimes not keep up. There are
//! three ways to handle that and only one of them is honest:
//!
//! - **Queue every tick.** Unbounded memory growth on a monitor, which is the
//!   one program that must not be the reason a machine fell over.
//! - **Drop silently and keep the newest.** Correct for a *screen* — nobody
//!   wants a stale frame — and quietly wrong for a *table*. A gap in stored
//!   metrics is indistinguishable from a machine that was idle, or off, or
//!   unplugged, which is exactly the "no data versus zero" confusion the schema
//!   exists to prevent, arriving through the back door.
//! - **Drop the newest-but-one and count what was dropped.** What this does.
//!
//! [`FleetSink::skipped`] is the count, and it is not decoration: a caller that
//! never reads it has a table with unexplained holes. The count is the
//! difference between "this machine reported nothing between 02:00 and 03:00"
//! and "this machine was too busy to write between 02:00 and 03:00", and those
//! are opposite conclusions for a capacity planner.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use tokio::sync::Notify;

use crate::pipeline::{PublishHook, Snapshot, SnapshotHandle};

use super::FleetStore;

/// Where the sink reads the newest snapshot from.
///
/// A closure rather than a [`SnapshotHandle`] directly, for two reasons. It is
/// the smaller thing to depend on — the sink needs "give me the latest", not the
/// interval controls or the stop flag — and a `SnapshotHandle` has no public
/// constructor, so depending on it would make this module testable only by
/// running a real collector.
pub type SnapshotSource = Arc<dyn Fn() -> Arc<Snapshot> + Send + Sync>;

/// Appends published snapshots to a [`FleetStore`], off the collector thread.
pub struct FleetSink {
    store: FleetStore,
    snapshots: SnapshotSource,
    doorbell: Arc<Notify>,
    /// Generations that were published but never stored, because the append for
    /// an earlier one was still running. See the module header.
    skipped: Arc<AtomicU64>,
    /// Generations actually written.
    stored: Arc<AtomicU64>,
    /// The last generation written, so a tick that arrives twice is not stored
    /// twice.
    last_stored: AtomicU64,
}

/// What a sink reports about its own keeping-up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SinkStats {
    /// Generations written to the table.
    pub stored: u64,
    /// Generations published while an append was in flight, and therefore never
    /// written. **Holes in the table**, not idle time.
    pub skipped: u64,
}

impl FleetSink {
    /// Build a sink over a store and the collector's snapshot handle.
    pub fn from_handle(store: FleetStore, handle: SnapshotHandle) -> Self {
        Self::new(store, Arc::new(move || handle.latest()))
    }

    /// Build a sink over a store and anything that can yield the newest
    /// snapshot.
    pub fn new(store: FleetStore, snapshots: SnapshotSource) -> Self {
        Self {
            store,
            snapshots,
            doorbell: Arc::new(Notify::new()),
            skipped: Arc::new(AtomicU64::new(0)),
            stored: Arc::new(AtomicU64::new(0)),
            last_stored: AtomicU64::new(0),
        }
    }

    /// The hook to hand to [`crate::pipeline::CollectorConfig::on_publish`].
    ///
    /// Does one atomic store and one `notify_one`. Nothing here allocates,
    /// locks, or touches the filesystem, because it runs on the collector.
    pub fn publish_hook(&self) -> PublishHook {
        let doorbell = Arc::clone(&self.doorbell);
        Arc::new(move || doorbell.notify_one())
    }

    /// What this sink has written and what it has missed.
    pub fn stats(&self) -> SinkStats {
        SinkStats {
            stored: self.stored.load(Ordering::Relaxed),
            skipped: self.skipped.load(Ordering::Relaxed),
        }
    }

    /// Append the newest snapshot, if it is one this sink has not stored.
    ///
    /// Returns the number of rows written. Separate from [`Self::run`] so that a
    /// caller — and a test — can drive one append and see its result, rather
    /// than only being able to start a loop and watch statistics move.
    pub async fn append_latest(&self) -> Result<usize, iceberg::Error> {
        let snapshot = (self.snapshots)();
        let generation = snapshot.generation;

        // Already stored, or nothing new since the last pass. Not an error and
        // not a skip: no data was lost, there simply is none.
        if generation != 0 && generation <= self.last_stored.load(Ordering::Relaxed) {
            return Ok(0);
        }

        let rows = self.store.append(&snapshot).await?;
        if rows > 0 {
            // Any generation between the last stored and this one was published
            // while the previous append was running, and is gone. Counting the
            // gap rather than the events means a burst is reported accurately
            // even though nothing observed each individual loss.
            let previous = self.last_stored.swap(generation, Ordering::Relaxed);
            if previous != 0 && generation > previous + 1 {
                self.skipped
                    .fetch_add(generation - previous - 1, Ordering::Relaxed);
            }
            self.stored.fetch_add(1, Ordering::Relaxed);
        }
        Ok(rows)
    }

    /// Append on every publish until `stop` resolves.
    ///
    /// Errors are returned rather than logged-and-continued, on the reasoning in
    /// [`super::store`]: a store that silently drops ticks produces a table with
    /// holes that read as idle time.
    pub async fn run(
        &self,
        mut stop: tokio::sync::oneshot::Receiver<()>,
    ) -> Result<SinkStats, iceberg::Error> {
        loop {
            tokio::select! {
                _ = self.doorbell.notified() => {
                    self.append_latest().await?;
                }
                _ = &mut stop => return Ok(self.stats()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fleet_store::store::test_support::in_memory_store;
    use arc_swap::ArcSwap;

    /// A snapshot source a test can move forward by hand.
    fn source(initial: Snapshot) -> (SnapshotSource, Arc<ArcSwap<Snapshot>>) {
        let slot = Arc::new(ArcSwap::from(Arc::new(initial)));
        let reader = Arc::clone(&slot);
        (Arc::new(move || reader.load_full()), slot)
    }

    /// The hook must be cheap enough to run on the collector: it may not block,
    /// so the only thing it is allowed to do is signal.
    #[tokio::test]
    async fn the_publish_hook_only_signals() {
        let (_warehouse, store) = in_memory_store("hook").await;
        let (src, _slot) = source(Snapshot::default());
        let sink = FleetSink::new(store, src);

        let hook = sink.publish_hook();
        // Must return immediately, and must not panic with no listener attached.
        hook();
        hook();

        assert_eq!(
            sink.stats(),
            SinkStats {
                stored: 0,
                skipped: 0
            },
            "ringing the bell must not itself write anything"
        );
    }

    /// A gap in generations is counted, because a hole in the table is not idle
    /// time and nothing else in the system can tell the difference later.
    #[tokio::test]
    async fn generations_missed_while_appending_are_counted() {
        let (_warehouse, store) = in_memory_store("gap").await;
        let (src, slot) = source(Snapshot {
            generation: 1,
            collected_at: 1_700_000_000,
            ..Default::default()
        });
        let sink = FleetSink::new(store, src);

        sink.append_latest().await.expect("first");
        assert_eq!(sink.stats().stored, 1);
        assert_eq!(sink.stats().skipped, 0);

        // Generations 2 and 3 are published while the first append is notionally
        // still running; only 4 is ever observed.
        slot.store(Arc::new(Snapshot {
            generation: 4,
            collected_at: 1_700_000_003,
            ..Default::default()
        }));
        sink.append_latest().await.expect("second");

        assert_eq!(sink.stats().stored, 2);
        assert_eq!(
            sink.stats().skipped,
            2,
            "generations 2 and 3 never reached the table, and the table cannot say so itself"
        );
    }

    /// The same generation twice must not produce two rows for one instant.
    #[tokio::test]
    async fn the_same_generation_is_not_stored_twice() {
        let (_warehouse, store) = in_memory_store("dup").await;
        let (src, _slot) = source(Snapshot {
            generation: 9,
            collected_at: 1_700_000_000,
            ..Default::default()
        });
        let sink = FleetSink::new(store, src);

        assert_eq!(sink.append_latest().await.expect("first"), 1);
        assert_eq!(
            sink.append_latest().await.expect("second"),
            0,
            "a re-notified tick must not duplicate a row"
        );
        assert_eq!(sink.stats().stored, 1);
        assert_eq!(
            sink.stats().skipped,
            0,
            "nothing was lost, so nothing may be reported as lost"
        );
    }
}
