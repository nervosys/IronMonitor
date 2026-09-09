//! Lock-free snapshot pipeline: decouples hardware collection from rendering.
//!
//! # Why this exists
//!
//! Before this module, every frontend (CLI, TUI, GUI) called blocking collectors
//! directly on its render thread. A slow WMI or NVML query would stall a frame,
//! and [`crate::backend::MonitoringBackend::update`] ran every collector serially,
//! so the slowest source set the floor for all of them.
//!
//! This module inverts that:
//!
//! - A dedicated **collector thread** owns all hardware handles and never shares them.
//! - Independent collectors run **concurrently** within each tick, so total tick cost
//!   is the slowest single collector rather than their sum.
//! - Results are published as an immutable [`Snapshot`] into an [`ArcSwap`] slot.
//!   Readers do a lock-free atomic load and never block, never wait on a mutex, and
//!   never touch a hardware API.
//!
//! # Thread-safety design
//!
//! [`crate::backend::MonitoringBackend`] cannot be moved across threads: it stores
//! `Vec<Box<dyn DiskDevice>>` and `Vec<Box<dyn MotherboardDevice>>`, trait objects
//! with no `Send` bound. Rather than retrofit `Send` onto those traits, the collector
//! thread **constructs its own sources** and only ever sends plain owned data
//! ([`Snapshot`]) across the boundary. Nothing that touches a driver ever crosses a
//! thread.
//!
//! On Windows this also matters for COM: `wmi` initializes COM per-thread, and
//! `disk::windows` contains a `COMLibrary::assume_initialized()` path that is only
//! sound on a thread where COM was actually initialized. Every thread this module
//! spawns therefore calls [`com_guard`] first and holds the guard for its lifetime.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use arc_swap::ArcSwap;
use serde::{Deserialize, Serialize};

use crate::connections::{ConnectionInfo, ConnectionMonitor};
use crate::core::cpu::CpuStats;
use crate::core::memory::MemoryStats;
use crate::gpu::{GpuCollection, GpuDynamicInfo, GpuStaticInfo};
use crate::network_monitor::NetworkMonitor;
use crate::process_monitor::{ProcessMonitor, ProcessMonitorInfo};
use crate::system_stats::SystemStats;

/// Default collector tick interval.
pub const DEFAULT_TICK: Duration = Duration::from_millis(1000);

/// Default number of samples retained per history series.
pub const DEFAULT_HISTORY: usize = 120;

// ============================================================================
// COM initialization guard
// ============================================================================

/// Per-thread COM guard.
///
/// On Windows this initializes COM for the calling thread and keeps it initialized
/// for as long as the returned value is held. Every thread that may reach a WMI
/// collector must hold one, otherwise `COMLibrary::assume_initialized()` in
/// `disk::windows` is unsound.
///
/// On non-Windows targets this is a zero-sized no-op.
#[cfg(target_os = "windows")]
pub fn com_guard() -> Option<wmi::COMLibrary> {
    wmi::COMLibrary::new().ok()
}

/// Per-thread COM guard (no-op on non-Windows targets).
///
/// Returns `Option<()>` rather than `()` so the five call sites can keep binding
/// the result to hold the guard for the scope, as they must on Windows. Returning
/// unit made every one of those a `let _x = ()`, which clippy rejects — and the
/// fix belongs here rather than in five cfg'd bindings at the call sites.
#[cfg(not(target_os = "windows"))]
pub fn com_guard() -> Option<()> {
    None
}

// ============================================================================
// Plain-data snapshot types
// ============================================================================

/// A disk row, flattened to plain data so it can cross a thread boundary.
///
/// `Box<dyn DiskDevice>` is not `Send`, so the collector flattens it here.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DiskSnapshot {
    /// Device model or drive letter.
    pub name: String,
    /// Mount point or drive root.
    pub mount_point: String,
    /// Total capacity in bytes.
    pub total: u64,
    /// Used capacity in bytes.
    pub used: u64,
    /// Filesystem label (NTFS, ext4, ...).
    pub filesystem: String,
    /// Read throughput in bytes/sec, or `None` where no rate was established.
    ///
    /// This was `f64` and it was **zero on every platform**. Windows takes the
    /// cheap `logical_drives` path, which reads capacity and no counters at
    /// all, and hardcoded `0.0`; the other platforms wrote
    /// `io.read_throughput.unwrap_or(0)`, and `read_throughput` is itself
    /// always `None` because a rate needs two samples to difference and
    /// `DiskIoStats` is built from one.
    ///
    /// So every consumer -- the TUI, the GUI, and
    /// `ironmon_disk_read_bytes_per_sec` -- has been drawing a flat line at zero
    /// and calling it disk throughput.
    pub read_rate: Option<f64>,
    /// Write throughput in bytes/sec. See [`Self::read_rate`].
    pub write_rate: Option<f64>,
}

/// Cumulative I/O counters for one physical device.
///
/// Kept separate from [`DiskSnapshot`] rather than folded into it, because the
/// two are keyed differently and joining them is its own problem.
/// `DiskSnapshot` is per filesystem — a drive letter or mount point — and these
/// counters are per physical drive; on Windows one physical drive can carry
/// several letters and one letter can span drives, so attributing a device
/// counter to a mount point needs a partition map nobody has written.
///
/// Publishing them under their own device label sidesteps that entirely, and is
/// what the metric means anyway: `ironmon_disk_read_bytes_total{device="..."}` is
/// a property of the hardware, not of a mount.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiskIoSnapshot {
    /// Device name, e.g. `PhysicalDrive0` or `nvme0n1`.
    pub device: String,
    /// Bytes read from this device since boot.
    pub read_bytes: u64,
    /// Bytes written to this device since boot.
    pub write_bytes: u64,
}

/// A network interface row with computed bandwidth rates.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NetSnapshot {
    /// Interface name.
    pub name: String,
    /// Interface is up.
    pub is_up: bool,
    /// Cumulative bytes received.
    pub rx_bytes: u64,
    /// Cumulative bytes transmitted.
    pub tx_bytes: u64,
    /// Receive rate in bytes/sec.
    /// Receive rate, or `None` until a second sample establishes one.
    ///
    /// A rate is a difference between two readings. This was a bare
    /// number and the first reading reported `0`, which says the link
    /// is idle -- and because the baseline was overwritten before the
    /// subtraction, every reading was the first reading.
    pub rx_rate: Option<f64>,
    /// Transmit rate in bytes/sec.
    /// Transmit rate. See [`Self::rx_rate`].
    pub tx_rate: Option<f64>,
    /// Link speed in Mbps, if known.
    pub speed_mbps: Option<u32>,
}

/// Per-collector wall-clock cost for the most recent tick, in microseconds.
///
/// Exposed so the TUI/GUI can surface which source is the bottleneck, and so
/// regressions in collection cost are observable rather than guessed at.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageTimings {
    /// CPU collection cost.
    pub cpu_us: u64,
    /// Memory collection cost.
    pub memory_us: u64,
    /// GPU collection cost.
    pub gpu_us: u64,
    /// Process enumeration cost.
    pub process_us: u64,
    /// Network collection cost.
    pub network_us: u64,
    /// Connection table cost.
    pub connection_us: u64,
    /// Disk enumeration cost.
    pub disk_us: u64,
    /// System stats cost.
    pub system_us: u64,
}

impl StageTimings {
    /// Cost of the slowest single collector — the theoretical floor for a tick
    /// once collectors run concurrently.
    pub fn critical_path_us(&self) -> u64 {
        [
            self.cpu_us,
            self.memory_us,
            self.gpu_us,
            self.process_us,
            self.network_us,
            self.connection_us,
            self.disk_us,
            self.system_us,
        ]
        .into_iter()
        .max()
        .unwrap_or(0)
    }

    /// Cost if every collector had run serially — what the old `update()` paid.
    pub fn serial_us(&self) -> u64 {
        self.cpu_us
            + self.memory_us
            + self.gpu_us
            + self.process_us
            + self.network_us
            + self.connection_us
            + self.disk_us
            + self.system_us
    }
}

/// Ring-buffer history series, stored as plain `Vec` so the snapshot stays trivially
/// cloneable and serializable.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Histories {
    /// CPU utilization percentage over time.
    pub cpu: Vec<f32>,
    /// Memory utilization percentage over time.
    pub memory: Vec<f32>,
    /// Per-GPU utilization percentage over time.
    pub gpu_util: Vec<Vec<f32>>,
    /// Per-GPU memory utilization percentage over time.
    pub gpu_memory: Vec<Vec<f32>>,
    /// Per-GPU temperature over time.
    pub gpu_temp: Vec<Vec<f32>>,
    /// Aggregate network receive rate over time.
    pub net_rx: Vec<f32>,
    /// Aggregate network transmit rate over time.
    pub net_tx: Vec<f32>,
}

/// An immutable, self-contained view of system state at one instant.
///
/// Cheap to share: readers hold an `Arc<Snapshot>` and never copy the contents.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Snapshot {
    /// Monotonically increasing tick counter. Readers compare this against a cached
    /// value to decide whether any recomputation (formatting, layout) is needed.
    pub generation: u64,
    /// Wall-clock capture time, seconds since the Unix epoch.
    pub collected_at: u64,
    /// Total wall-clock cost of this tick.
    pub collect_us: u64,
    /// Per-collector breakdown of this tick.
    pub timings: StageTimings,

    /// CPU statistics, if the platform collector succeeded.
    pub cpu: Option<CpuStats>,
    /// Memory statistics, if the platform collector succeeded.
    pub memory: Option<MemoryStats>,
    /// Static GPU descriptors, captured once at startup.
    pub gpu_static: Vec<GpuStaticInfo>,
    /// Per-tick GPU telemetry, index-aligned with [`Snapshot::gpu_static`].
    ///
    /// `None` marks a device whose query failed this tick. Failures keep their slot
    /// rather than being dropped, so a flaky device cannot silently renumber the
    /// GPUs beside it.
    pub gpu_dynamic: Vec<Option<GpuDynamicInfo>>,
    /// Process table with GPU attribution.
    pub processes: Vec<ProcessMonitorInfo>,
    /// Open network connections.
    pub connections: Vec<ConnectionInfo>,
    /// Mounted disks.
    pub disks: Vec<DiskSnapshot>,
    /// Cumulative I/O counters, per physical device. See [`DiskIoSnapshot`].
    pub disk_io: Vec<DiskIoSnapshot>,
    /// CPU temperature sensors, one entry per readable sensor.
    ///
    /// Empty where the CPU exposes none, which is a common case rather than a
    /// failure: this crate's own development machine reads zero sensors through
    /// every one of the four Windows paths `hwmon` tries. Empty means "nothing
    /// to report", never "0 degrees".
    pub cpu_temperatures: Vec<crate::hwmon::HwSensor>,
    /// Active network interfaces.
    pub network: Vec<NetSnapshot>,
    /// Load average / vmstat style system counters.
    pub system_stats: Option<SystemStats>,
    /// Retained time series.
    pub histories: Histories,
}

impl Snapshot {
    /// CPU utilization percentage, or 0.0 when unavailable.
    pub fn cpu_utilization(&self) -> Option<f32> {
        self.cpu.as_ref().map(|c| 100.0 - c.total.idle)
    }

    /// Memory utilization percentage, or `None` when memory was not read.
    /// See [`Self::cpu_utilization`].
    pub fn memory_utilization(&self) -> Option<f32> {
        self.memory.as_ref().map(|m| m.ram_usage_percent())
    }

    /// Aggregate receive rate across all interfaces, in bytes/sec.
    /// Receive rate summed over the interfaces that have one.
    ///
    /// `None` while no interface has established a rate yet, rather than `0.0`:
    /// a total over nothing measured is not a measurement of nothing.
    pub fn total_rx_rate(&self) -> Option<f64> {
        let rates: Vec<f64> = self.network.iter().filter_map(|n| n.rx_rate).collect();
        (!rates.is_empty()).then(|| rates.iter().sum())
    }

    /// Aggregate transmit rate across the interfaces that have one.
    ///
    /// `None` while none has been established. See [`Self::total_rx_rate`].
    pub fn total_tx_rate(&self) -> Option<f64> {
        let rates: Vec<f64> = self.network.iter().filter_map(|n| n.tx_rate).collect();
        (!rates.is_empty()).then(|| rates.iter().sum())
    }
}

// ============================================================================
// Reader handle
// ============================================================================

/// A cheap, cloneable reader handle onto the newest [`Snapshot`].
///
/// Cloning is refcount-only. [`SnapshotHandle::latest`] is a lock-free atomic load,
/// safe to call every frame from any number of threads.
#[derive(Clone)]
pub struct SnapshotHandle {
    slot: Arc<ArcSwap<Snapshot>>,
    stop: Arc<AtomicBool>,
    interval_ms: Arc<AtomicU64>,
}

impl SnapshotHandle {
    /// Load the newest snapshot. Lock-free; never blocks on the collector.
    pub fn latest(&self) -> Arc<Snapshot> {
        self.slot.load_full()
    }

    /// Generation of the newest snapshot, without cloning the `Arc`.
    ///
    /// Use this to skip work when nothing has changed since the last frame.
    pub fn generation(&self) -> u64 {
        self.slot.load().generation
    }

    /// Change the collector tick interval. Takes effect on the next tick.
    pub fn set_interval(&self, interval: Duration) {
        self.interval_ms
            .store(interval.as_millis() as u64, Ordering::Relaxed);
    }

    /// Current collector tick interval.
    pub fn interval(&self) -> Duration {
        Duration::from_millis(self.interval_ms.load(Ordering::Relaxed))
    }

    /// Ask the collector thread to shut down after its current tick.
    pub fn request_stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    /// Whether a stop has been requested.
    pub fn is_stopping(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }
}

// ============================================================================
// Collector
// ============================================================================

/// Called on the collector thread immediately after each snapshot is published.
///
/// Must not block: it runs inline on the collector between the store and the next
/// tick's sleep. Waking a UI is the intended use.
pub type PublishHook = Arc<dyn Fn() + Send + Sync>;

/// Collector configuration.
#[derive(Clone)]
pub struct CollectorConfig {
    /// Interval between ticks.
    pub interval: Duration,
    /// Samples retained per history series.
    pub history_size: usize,
    /// Collect the process table. Disable for lightweight headless use.
    pub collect_processes: bool,
    /// Collect the open-connection table (relatively expensive on Windows).
    pub collect_connections: bool,
    /// Re-enumerate disks every N ticks. Disk topology changes rarely, so
    /// re-enumerating every tick wastes a comparatively expensive WMI call.
    pub disk_every_n_ticks: u64,
    /// Re-read the process table every N ticks, reusing the previous table between.
    ///
    /// This is the most expensive stage by a wide margin — 970 ms for 484 processes
    /// on a Windows box, because each entry costs an `OpenProcess` plus a SID lookup.
    /// Front-ends display it far less often than they display CPU and GPU, so paying
    /// for it every tick pins a core for data nobody is looking at.
    pub process_every_n_ticks: u64,
    /// Re-read the open-connection table every N ticks.
    pub connection_every_n_ticks: u64,
    /// Largest share of wall-clock time collection may occupy, in `0.05..=1.0`.
    ///
    /// The collector sleeps until at least `tick_cost / max_duty_cycle` has passed,
    /// so a tick that costs more than the interval allows stretches the interval
    /// instead of pinning a core. At the 0.5 default a 500ms tick still publishes
    /// every second; a 1.5s tick publishes every three seconds rather than
    /// continuously.
    pub max_duty_cycle: f32,
    /// Invoked after every publish, on the collector thread.
    ///
    /// Without this a front-end has to poll: it wakes on a timer, checks the
    /// generation, and usually finds nothing. Polling at a fraction of the tick makes
    /// the observed update interval jitter by that fraction — the display advances
    /// twice in quick succession, then appears to stall — and the jitter is worst
    /// when tick cost varies, which is exactly when a machine is busy. A hook lets
    /// the collector wake the reader at the instant data lands, so the visible
    /// cadence is the collector's cadence.
    pub on_publish: Option<PublishHook>,
}

impl std::fmt::Debug for CollectorConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CollectorConfig")
            .field("interval", &self.interval)
            .field("history_size", &self.history_size)
            .field("collect_processes", &self.collect_processes)
            .field("collect_connections", &self.collect_connections)
            .field("disk_every_n_ticks", &self.disk_every_n_ticks)
            .field("process_every_n_ticks", &self.process_every_n_ticks)
            .field("connection_every_n_ticks", &self.connection_every_n_ticks)
            .field("max_duty_cycle", &self.max_duty_cycle)
            .field("on_publish", &self.on_publish.is_some())
            .finish()
    }
}

impl Default for CollectorConfig {
    fn default() -> Self {
        Self {
            interval: DEFAULT_TICK,
            history_size: DEFAULT_HISTORY,
            collect_processes: true,
            collect_connections: true,
            disk_every_n_ticks: 10,
            process_every_n_ticks: 1,
            connection_every_n_ticks: 1,
            max_duty_cycle: 0.5,
            on_publish: None,
        }
    }
}

/// Owns the collector thread. Dropping this stops and joins the thread.
pub struct Collector {
    handle: SnapshotHandle,
    join: Option<JoinHandle<()>>,
}

impl Collector {
    /// Spawn the collector thread and return immediately.
    ///
    /// The thread constructs its own hardware handles, so no driver state is ever
    /// shared. The first snapshot may briefly be [`Snapshot::default`] until the
    /// first tick lands; readers should treat generation 0 as "not ready yet".
    pub fn spawn(config: CollectorConfig) -> Self {
        let slot = Arc::new(ArcSwap::from_pointee(Snapshot::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let interval_ms = Arc::new(AtomicU64::new(config.interval.as_millis() as u64));

        let handle = SnapshotHandle {
            slot: Arc::clone(&slot),
            stop: Arc::clone(&stop),
            interval_ms: Arc::clone(&interval_ms),
        };

        let join = thread::Builder::new()
            .name("ironmon-collector".into())
            .spawn(move || collector_loop(config, slot, stop, interval_ms))
            .ok();

        Self { handle, join }
    }

    /// A cloneable reader handle for the UI threads.
    pub fn handle(&self) -> SnapshotHandle {
        self.handle.clone()
    }
}

impl Drop for Collector {
    fn drop(&mut self) {
        self.handle.request_stop();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

/// Hardware handles owned exclusively by the collector thread.
struct Sources {
    gpu: Option<GpuCollection>,
    processes: Option<ProcessMonitor>,
    network: Option<NetworkMonitor>,
    connections: Option<ConnectionMonitor>,
}

/// Bounded push that keeps the newest `cap` samples.
fn push_capped(series: &mut Vec<f32>, value: f32, cap: usize) {
    if series.len() >= cap {
        let overflow = series.len() + 1 - cap;
        series.drain(..overflow);
    }
    series.push(value);
}

fn collector_loop(
    config: CollectorConfig,
    slot: Arc<ArcSwap<Snapshot>>,
    stop: Arc<AtomicBool>,
    interval_ms: Arc<AtomicU64>,
) {
    // COM must be initialized on this thread before any WMI collector runs, and
    // must stay initialized for the thread's lifetime.
    com_guard();

    let mut histories = Histories::default();
    let mut generation: u64 = 0;
    let mut cached_disks: Vec<DiskSnapshot> = Vec::new();
    let mut cached_disk_io: Vec<DiskIoSnapshot> = Vec::new();
    // Carried between ticks so a decimated stage keeps its last reading rather than
    // publishing an empty table on the ticks it does not run.
    let mut cached_processes: Vec<ProcessMonitorInfo> = Vec::new();
    let mut cached_connections: Vec<ConnectionInfo> = Vec::new();

    // === Warm-up publish ===
    //
    // Constructing the hardware sources is slow: `GpuCollection::auto_detect` spins
    // up vendor drivers (NVML, DXGI) and, on a multi-GPU host, measured 8-12s before
    // it returned. Publishing nothing until it finished left every UI blank for that
    // whole window, even though CPU and memory were available in under a millisecond.
    //
    // So publish those first. `collect_once` already treats every source as optional,
    // so passing an empty `Sources` yields exactly the collectors that need no setup
    // — CPU, memory, system stats and disks — and leaves the rest empty until the
    // real sources exist. Consumers key off `generation`, so this simply arrives as
    // the first update and is superseded a moment later.
    let mut warmup_sources = Sources {
        gpu: None,
        processes: None,
        network: None,
        connections: None,
    };
    generation += 1;
    slot.store(Arc::new(collect_once(
        &mut warmup_sources,
        &config,
        &mut histories,
        &mut cached_disks,
        &mut cached_disk_io,
        &mut cached_processes,
        &mut cached_connections,
        &[],
        generation,
    )));
    if let Some(ref hook) = config.on_publish {
        hook();
    }

    if stop.load(Ordering::Relaxed) {
        return;
    }

    let mut sources = Sources {
        gpu: GpuCollection::auto_detect().ok(),
        processes: if config.collect_processes {
            ProcessMonitor::new().ok()
        } else {
            None
        },
        network: NetworkMonitor::new().ok(),
        connections: if config.collect_connections {
            ConnectionMonitor::new().ok()
        } else {
            None
        },
    };

    // Static GPU descriptors are captured once; they do not change at runtime.
    let gpu_static: Vec<GpuStaticInfo> = sources
        .gpu
        .as_ref()
        .and_then(|g| g.snapshot_all().ok())
        .map(|infos| infos.iter().map(|i| i.static_info.clone()).collect())
        .unwrap_or_default();

    // Size the per-GPU series now that the device count is known. The warm-up pass
    // ran with none, so these start empty.
    histories.gpu_util = vec![Vec::new(); gpu_static.len()];
    histories.gpu_memory = vec![Vec::new(); gpu_static.len()];
    histories.gpu_temp = vec![Vec::new(); gpu_static.len()];

    while !stop.load(Ordering::Relaxed) {
        let tick_start = Instant::now();
        generation += 1;

        let snapshot = collect_once(
            &mut sources,
            &config,
            &mut histories,
            &mut cached_disks,
            &mut cached_disk_io,
            &mut cached_processes,
            &mut cached_connections,
            &gpu_static,
            generation,
        );

        slot.store(Arc::new(snapshot));
        if let Some(ref hook) = config.on_publish {
            hook();
        }

        // Sleep the remainder of the interval, re-reading it each tick so the UI can
        // retune cadence live. Waking in short slices keeps shutdown responsive even
        // at long intervals.
        let interval = Duration::from_millis(interval_ms.load(Ordering::Relaxed).max(50));
        let elapsed = tick_start.elapsed();

        // Never let collection occupy more than `max_duty_cycle` of wall time.
        //
        // Collection cost is not a constant of the machine — it is a function of how
        // busy the machine is. NVML queries against a GPU running sustained compute
        // took 515ms here versus 1.6ms for an idle card of the same model, and the
        // WMI GPU counter classes slow down under load the same way. A fixed interval
        // therefore behaves worst exactly when the user is watching: the collector
        // takes a larger and larger share of a core, and because each tick overruns
        // by a different amount the publishes stop being evenly spaced.
        //
        // Backing off in proportion to what the last tick actually cost bounds the
        // load a monitor may impose and keeps the spacing predictable. When
        // collection is cheap — the common case — the configured interval is
        // unchanged and this does nothing.
        let min_period = elapsed.div_f32(config.max_duty_cycle.clamp(0.05, 1.0));
        let period = interval.max(min_period);
        let mut remaining = period.saturating_sub(elapsed);
        while remaining > Duration::ZERO && !stop.load(Ordering::Relaxed) {
            let slice = remaining.min(Duration::from_millis(50));
            thread::sleep(slice);
            remaining = remaining.saturating_sub(slice);
        }
    }
}

/// Run one tick, fanning independent collectors out across scoped threads.
///
/// Each closure borrows a *disjoint* field of [`Sources`], which is what makes
/// concurrent `&mut` access sound here. Every scoped thread takes its own COM guard.
// Each argument is a distinct borrowed subsystem; bundling them into a struct would
// reintroduce the aliasing the disjoint-field borrows exist to avoid.
#[allow(clippy::too_many_arguments)]
fn collect_once(
    sources: &mut Sources,
    config: &CollectorConfig,
    histories: &mut Histories,
    cached_disks: &mut Vec<DiskSnapshot>,
    cached_disk_io: &mut Vec<DiskIoSnapshot>,
    cached_processes: &mut Vec<ProcessMonitorInfo>,
    cached_connections: &mut Vec<ConnectionInfo>,
    gpu_static: &[GpuStaticInfo],
    generation: u64,
) -> Snapshot {
    let started = Instant::now();

    // Destructure so each scoped thread captures a distinct field.
    let Sources {
        gpu,
        processes,
        network,
        connections,
    } = sources;

    // The first tick always collects everything so the UI is populated immediately;
    // after that each decimated stage runs on its own cadence and the previous value
    // is reused in between.
    let due = |every: u64| generation == 1 || generation.is_multiple_of(every.max(1));
    let refresh_disks = due(config.disk_every_n_ticks);
    let refresh_processes = due(config.process_every_n_ticks);
    let refresh_connections = due(config.connection_every_n_ticks);

    #[allow(clippy::type_complexity)]
    let (cpu, memory, gpu_dynamic, process_list, net_list, conn_list, disk_list, sys_stats, temps) =
        thread::scope(|scope| {
            let cpu_h = scope.spawn(timed(collect_cpu));
            // ~2 ms per call after a one-time ~365 ms initialisation, so it sits
            // on the per-tick cadence with the CPU reading it belongs beside.
            let temp_h = scope.spawn(timed(crate::hwmon::read_cpu_temperatures));
            let mem_h = scope.spawn(timed(collect_memory));
            let sys_h = scope.spawn(timed(collect_system_stats));
            let gpu_h = scope.spawn(timed(move || collect_gpu(gpu.as_ref())));
            let proc_h = scope.spawn(timed(move || {
                refresh_processes.then(|| collect_processes(processes.as_mut()))
            }));
            let net_h = scope.spawn(timed(move || collect_network(network.as_mut())));
            let conn_h = scope.spawn(timed(move || {
                refresh_connections.then(|| collect_connections(connections.as_mut()))
            }));
            let disk_h = scope.spawn(timed(move || {
                if refresh_disks {
                    Some((collect_disks(), collect_disk_io()))
                } else {
                    None
                }
            }));

            (
                join_or_default(cpu_h),
                join_or_default(mem_h),
                join_or_default(gpu_h),
                join_or_default(proc_h),
                join_or_default(net_h),
                join_or_default(conn_h),
                join_or_default(disk_h),
                join_or_default(sys_h),
                join_or_default(temp_h),
            )
        });

    let timings = StageTimings {
        cpu_us: cpu.1,
        memory_us: memory.1,
        gpu_us: gpu_dynamic.1,
        process_us: process_list.1,
        network_us: net_list.1,
        connection_us: conn_list.1,
        disk_us: disk_list.1,
        system_us: sys_stats.1,
    };

    let (cpu, memory) = (cpu.0, memory.0);
    let gpu_dynamic = gpu_dynamic.0;
    let network = net_list.0;
    let system_stats = sys_stats.0;

    if let Some((fresh_disks, fresh_io)) = disk_list.0 {
        *cached_disks = fresh_disks;
        *cached_disk_io = fresh_io;
    }
    if let Some(fresh) = process_list.0 {
        *cached_processes = fresh;
    }
    if let Some(fresh) = conn_list.0 {
        *cached_connections = fresh;
    }
    let processes = cached_processes.clone();
    let connections = cached_connections.clone();

    // === histories ===
    let cap = config.history_size.max(1);
    if let Some(ref c) = cpu {
        push_capped(&mut histories.cpu, 100.0 - c.total.idle, cap);
    }
    if let Some(ref m) = memory {
        push_capped(&mut histories.memory, m.ram_usage_percent(), cap);
    }
    for (i, info) in gpu_dynamic.iter().enumerate() {
        // A device that failed this tick contributes no sample; its series simply
        // does not advance, which reads as a flat segment rather than a false zero.
        let Some(info) = info.as_ref() else { continue };
        if i < histories.gpu_util.len() {
            if let Some(util) = info.utilization {
                push_capped(&mut histories.gpu_util[i], util as f32, cap);
            }
            // A device that reported no memory contributes no sample, exactly as
            // a device that failed its whole query contributes none above.
            if let Some(mem_util) = info.memory.utilization {
                push_capped(&mut histories.gpu_memory[i], mem_util as f32, cap);
            }
            if let Some(temp) = info.thermal.temperature {
                push_capped(&mut histories.gpu_temp[i], temp as f32, cap);
            }
        }
    }
    // Summed over the interfaces that have a rate. The history graph needs a
    // number per tick and the first tick after start-up has none -- pushing `0`
    // there draws a trough that never happened, so that sample is skipped and
    // the line starts one tick later.
    let rx_rates: Vec<f64> = network.iter().filter_map(|n| n.rx_rate).collect();
    let tx_rates: Vec<f64> = network.iter().filter_map(|n| n.tx_rate).collect();
    if !rx_rates.is_empty() {
        push_capped(
            &mut histories.net_rx,
            rx_rates.iter().sum::<f64>() as f32,
            cap,
        );
    }
    if !tx_rates.is_empty() {
        push_capped(
            &mut histories.net_tx,
            tx_rates.iter().sum::<f64>() as f32,
            cap,
        );
    }

    Snapshot {
        generation,
        collected_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        collect_us: started.elapsed().as_micros() as u64,
        timings,
        cpu,
        memory,
        gpu_static: gpu_static.to_vec(),
        gpu_dynamic,
        processes,
        connections,
        disks: cached_disks.clone(),
        disk_io: cached_disk_io.clone(),
        cpu_temperatures: temps.0,
        network,
        system_stats,
        histories: histories.clone(),
    }
}

/// Wrap a collector so it takes its own COM guard and reports its own wall-clock cost.
fn timed<T, F>(f: F) -> impl FnOnce() -> (T, u64) + Send
where
    F: FnOnce() -> T + Send,
    T: Send,
{
    move || {
        com_guard();
        let start = Instant::now();
        let value = f();
        (value, start.elapsed().as_micros() as u64)
    }
}

/// Join a scoped collector, treating a panic as "no data" rather than taking the
/// whole collector thread down with it. One flaky vendor driver must not kill
/// monitoring for everything else.
fn join_or_default<T: Default>(handle: thread::ScopedJoinHandle<'_, (T, u64)>) -> (T, u64) {
    handle.join().unwrap_or_else(|_| (T::default(), 0))
}

// ============================================================================
// Individual collectors
// ============================================================================

fn collect_cpu() -> Option<CpuStats> {
    #[cfg(target_os = "windows")]
    {
        crate::platform::windows::read_cpu_stats().ok()
    }
    #[cfg(target_os = "linux")]
    {
        crate::platform::linux::read_cpu_stats().ok()
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        Some(CpuStats::empty())
    }
}

fn collect_memory() -> Option<MemoryStats> {
    #[cfg(target_os = "windows")]
    {
        crate::platform::windows::read_memory_stats().ok()
    }
    #[cfg(target_os = "linux")]
    {
        crate::platform::linux::read_memory_stats().ok()
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        Some(MemoryStats::empty())
    }
}

fn collect_system_stats() -> Option<SystemStats> {
    SystemStats::new().ok()
}

fn collect_gpu(gpu: Option<&GpuCollection>) -> Vec<Option<GpuDynamicInfo>> {
    let Some(collection) = gpu else {
        return Vec::new();
    };
    // Partial results keep index alignment with the static descriptors captured at
    // startup, and let one failing device degrade alone.
    collection
        .snapshot_all_partial()
        .into_iter()
        .map(|result| result.ok().map(|info| info.dynamic_info))
        .collect()
}

fn collect_processes(monitor: Option<&mut ProcessMonitor>) -> Vec<ProcessMonitorInfo> {
    monitor.and_then(|m| m.processes().ok()).unwrap_or_default()
}

fn collect_connections(monitor: Option<&mut ConnectionMonitor>) -> Vec<ConnectionInfo> {
    monitor
        .and_then(|m| m.all_connections().ok())
        .unwrap_or_default()
}

fn collect_network(monitor: Option<&mut NetworkMonitor>) -> Vec<NetSnapshot> {
    let Some(monitor) = monitor else {
        return Vec::new();
    };
    let Ok(interfaces) = monitor.interfaces() else {
        return Vec::new();
    };

    let is_virtual = |name: &str| {
        name.starts_with("lo")
            || name.contains("Loopback")
            || name.starts_with("vEthernet")
            || name.starts_with("Local Area Connection*")
            || name.starts_with("VMware")
            || name.starts_with("VirtualBox")
    };

    let mut out: Vec<NetSnapshot> = interfaces
        .iter()
        .filter(|i| !is_virtual(&i.name) && i.is_active())
        .map(|i| {
            let rate = monitor.bandwidth_rate(&i.name, i);
            NetSnapshot {
                name: i.name.clone(),
                is_up: i.is_up,
                rx_bytes: i.rx_bytes,
                tx_bytes: i.tx_bytes,
                rx_rate: rate.map(|(rx, _)| rx),
                tx_rate: rate.map(|(_, tx)| tx),
                speed_mbps: i.speed_mbps,
            }
        })
        .collect();

    // Fall back to any active non-loopback interface rather than showing nothing.
    if out.is_empty() {
        out = interfaces
            .iter()
            .filter(|i| !i.name.starts_with("lo") && !i.name.contains("Loopback") && i.is_active())
            .map(|i| {
                let rate = monitor.bandwidth_rate(&i.name, i);
                NetSnapshot {
                    name: i.name.clone(),
                    is_up: i.is_up,
                    rx_bytes: i.rx_bytes,
                    tx_bytes: i.tx_bytes,
                    rx_rate: rate.map(|(rx, _)| rx),
                    tx_rate: rate.map(|(_, tx)| tx),
                    speed_mbps: i.speed_mbps,
                }
            })
            .collect();
    }

    out
}

/// Cumulative per-device I/O counters.
///
/// One batched read rather than a call per disk. On Windows each `io_stats`
/// opens its own WMI connection, which measured 8.0 s across four drives
/// against 9.9 ms for the batch -- see `crate::disk::all_io_counters`. That cost
/// is why the Windows `collect_disks` takes the capacity-only `logical_drives`
/// path and why these counters were missing from the snapshot at all, which left
/// `ironmon_disk_read_bytes_total` unpublishable on the served endpoint.
///
/// Runs on the disk cadence (every 10 ticks by default), in the same stage.
fn collect_disk_io() -> Vec<DiskIoSnapshot> {
    let mut out: Vec<DiskIoSnapshot> = crate::disk::all_io_counters()
        .into_iter()
        .map(|(device, io)| DiskIoSnapshot {
            device,
            read_bytes: io.read_bytes,
            write_bytes: io.write_bytes,
        })
        .collect();
    // A map iterates in whatever order it likes; a snapshot should not shuffle
    // its rows between ticks.
    out.sort_by(|a, b| a.device.cmp(&b.device));
    out
}

#[cfg(target_os = "windows")]
fn collect_disks() -> Vec<DiskSnapshot> {
    // Windows: GetLogicalDriveStrings + GetDiskFreeSpaceExW is far cheaper and more
    // reliable than the WMI disk enumeration path.
    crate::platform::windows::logical_drives()
        .map(|drives| {
            drives
                .into_iter()
                .map(|d| DiskSnapshot {
                    name: d.name.clone(),
                    mount_point: d.name,
                    total: d.total,
                    used: d.used,
                    filesystem: d.filesystem,
                    // `logical_drives` reads capacity and no I/O counters, so
                    // there is no rate here to report. It said `0.0`.
                    read_rate: None,
                    write_rate: None,
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(not(target_os = "windows"))]
fn collect_disks() -> Vec<DiskSnapshot> {
    let Ok(disks) = crate::disk::enumerate_disks() else {
        return Vec::new();
    };

    disks
        .iter()
        .filter_map(|disk| {
            let info = disk.info().ok()?;

            let (mount_point, filesystem, used, total) = disk
                .filesystem_info()
                .ok()
                .and_then(|fs_infos| fs_infos.first().cloned())
                .map(|fs| {
                    (
                        fs.mount_point.to_string_lossy().to_string(),
                        fs.fs_type.clone(),
                        fs.used_size,
                        fs.total_size,
                    )
                })
                .unwrap_or_else(|| ("N/A".to_string(), "N/A".to_string(), 0, info.capacity));

            let (read_rate, write_rate) = disk
                .io_stats()
                .ok()
                .map(|io| {
                    (
                        io.read_throughput.map(|v| v as f64),
                        io.write_throughput.map(|v| v as f64),
                    )
                })
                // A disk whose `io_stats` call failed reports no rate, rather
                // than a rate of zero.
                .unwrap_or((None, None));

            Some(DiskSnapshot {
                name: info.model,
                mount_point,
                total,
                used,
                filesystem,
                read_rate,
                write_rate,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sleep must bound collection's share of wall time, and must not shorten
    /// the configured interval when collection is cheap.
    ///
    /// This is the arithmetic from the collector loop. Collection cost rises with how
    /// busy the machine is — NVML against a loaded GPU measured 515ms versus 1.6ms
    /// for an idle card — so without a bound the collector takes its largest share of
    /// the machine exactly when the machine is already struggling.
    #[test]
    fn tick_period_bounds_collection_duty_cycle() {
        // Mirrors `collector_loop`: sleep until at least `elapsed / duty` has passed,
        // never publishing faster than the configured interval.
        let period = |elapsed_ms: u64, interval_ms: u64, duty: f32| -> Duration {
            let elapsed = Duration::from_millis(elapsed_ms);
            let interval = Duration::from_millis(interval_ms);
            interval.max(elapsed.div_f32(duty.clamp(0.05, 1.0)))
        };
        let ms = |d: Duration| d.as_millis() as u64;

        // Cheap tick: the configured interval is untouched.
        assert_eq!(ms(period(20, 1000, 0.5)), 1000);
        // Exactly at the bound: still the configured interval.
        assert_eq!(ms(period(500, 1000, 0.5)), 1000);
        // Expensive tick: the period stretches so collection stays at half the wall
        // clock rather than running back to back.
        assert_eq!(ms(period(1500, 1000, 0.5)), 3000);
        // A stricter budget stretches further.
        assert_eq!(ms(period(1000, 1000, 0.25)), 4000);

        // Whatever the inputs, the resulting duty cycle never exceeds the budget.
        // Compared in full Duration precision: the collector sleeps on Durations, so
        // rounding to whole milliseconds here would flag its own truncation.
        for elapsed_ms in [1u64, 50, 500, 1500, 5000] {
            for duty in [0.1f32, 0.25, 0.5, 1.0] {
                let p = period(elapsed_ms, 1000, duty).as_secs_f64();
                let ratio = Duration::from_millis(elapsed_ms).as_secs_f64() / p;
                assert!(
                    ratio <= duty as f64 + 1e-6,
                    "elapsed {elapsed_ms}ms over period {p}s gives duty {ratio}, over budget {duty}"
                );
            }
        }
    }

    #[test]
    fn push_capped_keeps_newest_samples() {
        let mut series = Vec::new();
        for i in 0..10 {
            push_capped(&mut series, i as f32, 4);
        }
        assert_eq!(series.len(), 4, "series must be bounded by cap");
        assert_eq!(series, vec![6.0, 7.0, 8.0, 9.0], "oldest samples evicted");
    }

    #[test]
    fn push_capped_handles_cap_of_one() {
        let mut series = Vec::new();
        push_capped(&mut series, 1.0, 1);
        push_capped(&mut series, 2.0, 1);
        assert_eq!(series, vec![2.0]);
    }

    #[test]
    fn critical_path_is_max_not_sum() {
        let t = StageTimings {
            cpu_us: 100,
            memory_us: 50,
            gpu_us: 900,
            process_us: 400,
            ..Default::default()
        };
        assert_eq!(
            t.critical_path_us(),
            900,
            "critical path is the slowest stage"
        );
        assert_eq!(t.serial_us(), 1450, "serial cost is the sum");
        assert!(
            t.critical_path_us() < t.serial_us(),
            "concurrent collection must beat serial"
        );
    }

    #[test]
    fn default_snapshot_is_generation_zero() {
        let snap = Snapshot::default();
        assert_eq!(snap.generation, 0, "readers treat gen 0 as not-ready");
        // Not `Some(0.0)`. A default snapshot has read nothing, and 0% CPU
        // is a claim that the machine is idle.
        assert_eq!(snap.cpu_utilization(), None);
        assert_eq!(snap.memory_utilization(), None);
    }

    #[test]
    fn snapshot_aggregates_network_rates() {
        let snap = Snapshot {
            network: vec![
                NetSnapshot {
                    rx_rate: Some(1.5),
                    tx_rate: Some(0.5),
                    ..Default::default()
                },
                NetSnapshot {
                    rx_rate: Some(2.5),
                    tx_rate: Some(1.5),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        assert_eq!(snap.total_rx_rate(), Some(4.0));
        assert_eq!(snap.total_tx_rate(), Some(2.0));
    }

    /// An interface with no established rate contributes nothing, and a
    /// snapshot where none has one totals to an absence rather than to zero.
    #[test]
    fn snapshot_totals_skip_interfaces_with_no_rate_yet() {
        let partial = Snapshot {
            network: vec![
                NetSnapshot {
                    rx_rate: Some(3.0),
                    tx_rate: None,
                    ..Default::default()
                },
                NetSnapshot {
                    rx_rate: None,
                    tx_rate: None,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        assert_eq!(partial.total_rx_rate(), Some(3.0));
        assert_eq!(partial.total_tx_rate(), None);

        let none_yet = Snapshot {
            network: vec![NetSnapshot::default()],
            ..Default::default()
        };
        assert_eq!(none_yet.total_rx_rate(), None);
    }

    #[test]
    fn handle_is_lock_free_readable_and_stoppable() {
        let slot = Arc::new(ArcSwap::from_pointee(Snapshot::default()));
        let handle = SnapshotHandle {
            slot: Arc::clone(&slot),
            stop: Arc::new(AtomicBool::new(false)),
            interval_ms: Arc::new(AtomicU64::new(1000)),
        };

        assert_eq!(handle.generation(), 0);
        slot.store(Arc::new(Snapshot {
            generation: 7,
            ..Default::default()
        }));
        assert_eq!(handle.generation(), 7, "readers see the newest publish");
        assert_eq!(handle.latest().generation, 7);

        handle.set_interval(Duration::from_millis(250));
        assert_eq!(handle.interval(), Duration::from_millis(250));

        assert!(!handle.is_stopping());
        handle.request_stop();
        assert!(handle.is_stopping());
    }

    /// The first published snapshot must already carry useful data.
    ///
    /// Constructing the GPU collection initializes vendor drivers and can take
    /// seconds. Before the warm-up publish, nothing at all was published until that
    /// finished, so every UI sat blank. This asserts the invariant that makes the
    /// difference — first data does not wait on driver enumeration — rather than an
    /// absolute millisecond bound, which would be flaky on a loaded machine.
    #[test]
    fn first_publish_does_not_wait_for_driver_enumeration() {
        let collector = Collector::spawn(CollectorConfig {
            interval: Duration::from_millis(200),
            ..Default::default()
        });
        let handle = collector.handle();

        let start = Instant::now();
        while handle.generation() == 0 && start.elapsed() < Duration::from_secs(60) {
            thread::sleep(Duration::from_millis(1));
        }

        let first = handle.latest();
        assert!(
            first.generation > 0,
            "collector published nothing within 60s"
        );
        assert!(
            first.cpu.is_some() || first.memory.is_some(),
            "the first snapshot carried no CPU or memory sample, so the warm-up \
             publish is not doing its job — consumers would render an empty UI \
             until driver enumeration completed"
        );
    }

    #[test]
    fn snapshot_is_send_and_sync() {
        // The whole design rests on Snapshot crossing threads; assert it statically
        // so a future field addition (e.g. an Rc or a trait object) fails the build
        // here rather than at the call site.
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Snapshot>();
        assert_send_sync::<SnapshotHandle>();
    }
}
