//! Bounded, collector-backed observations for agent tools.
//! Queries read the same generation instead of independently sampling hardware.

use super::{AiDataApi, ToolCategory, ToolDefinition};
use crate::error::{IronError, Result};
use crate::observability::{EventCategory, EventFilter, EventManager, EventSeverity, SystemEvent};
use crate::ontology::{resolve::Reading, Ontology, Provenance, ONTOLOGY_VERSION};
use crate::pipeline::{Collector, CollectorConfig, Snapshot, SnapshotHandle};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const HISTORY_BYTES: usize = 4 * 1024 * 1024;
const HISTORY_BATCHES: usize = 120;
const MAX_READINGS: usize = 512;
const MAX_EVENTS: usize = 256;
static GUI_SOURCE: OnceLock<SnapshotHandle> = OnceLock::new();
static SHARED: OnceLock<Arc<ObservationService>> = OnceLock::new();

#[cfg(feature = "gui")]
pub(crate) fn register_gui_source(source: SnapshotHandle) {
    let _ = GUI_SOURCE.set(source);
}

fn bad(message: impl Into<String>) -> IronError {
    IronError::Other(message.into())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn parse<T: serde::de::DeserializeOwned>(value: Value) -> Result<T> {
    serde_json::from_value(value).map_err(|e| bad(format!("Invalid tool parameters: {e}")))
}

fn bounded(value: u64, min: u64, max: u64, name: &str) -> Result<()> {
    if !(min..=max).contains(&value) {
        return Err(bad(format!("{name} must be in {min}..={max}")));
    }
    Ok(())
}

fn ids_valid(ids: &[String], max: usize) -> Result<()> {
    if ids.is_empty() || ids.len() > max {
        return Err(bad(format!(
            "ids must contain 1..={max} concrete entity IDs"
        )));
    }
    let ontology = ontology();
    for id in ids {
        if id.len() > 128 || Ontology::is_template(id) || ontology.template_for(id).is_none() {
            return Err(bad(format!(
                "No concrete entity with id {id:?}; consult ironmon describe"
            )));
        }
    }
    Ok(())
}

fn ontology() -> &'static Ontology {
    static ONTOLOGY: OnceLock<Ontology> = OnceLock::new();
    ONTOLOGY.get_or_init(Ontology::build)
}

#[derive(Clone)]
struct Observation {
    reading: Reading,
    sampled_at_ms: Option<u64>,
    derived_from: Vec<String>,
}

impl Serialize for Observation {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Output<'a> {
            #[serde(flatten)]
            reading: &'a Reading,
            sampled_at_ms: Option<u64>,
            sampled_at_utc: Option<String>,
            derived_from: &'a [String],
        }
        Output {
            reading: &self.reading,
            sampled_at_ms: self.sampled_at_ms,
            sampled_at_utc: self.sampled_at_ms.and_then(|ms| {
                i64::try_from(ms)
                    .ok()
                    .and_then(chrono::DateTime::from_timestamp_millis)
                    .map(|date| date.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
            }),
            derived_from: &self.derived_from,
        }
        .serialize(serializer)
    }
}

fn observation(id: String, value: Option<Value>, time: Option<u64>, reason: &str) -> Observation {
    let entity = ontology()
        .template_for(&id)
        .expect("collector mapping uses declared IDs");
    let mut note = None;
    let mut value = value.filter(|v| !v.is_null());
    if value
        .as_ref()
        .and_then(Value::as_str)
        .is_some_and(|text| text.len() > 1024)
    {
        value = None;
        note = Some("Text reading exceeded the 1024-byte retention limit and was withheld".into());
    }
    if let Some(v) = value.as_ref().and_then(Value::as_f64) {
        if let Some(problem) = entity.validate_range(v) {
            value = None;
            note = Some(format!("Withheld physically invalid reading: {problem}"));
        }
    }
    if value
        .as_ref()
        .and_then(Value::as_str)
        .is_some_and(crate::ontology::resolve::names_an_absence)
    {
        value = None;
        note = Some("The provider returned an absence marker rather than a reading".into());
    }
    let provenance = if value.is_none() {
        note.get_or_insert_with(|| reason.to_string());
        Provenance::Unavailable
    } else if !entity.derived_from.is_empty() {
        Provenance::Derived
    } else {
        entity.provenance
    };
    let derived_from = entity
        .derived_from
        .iter()
        .map(|input| {
            if let Some(instance) = id.split('.').nth(1) {
                Ontology::instantiate(input, instance)
            } else {
                input.clone()
            }
        })
        .collect();
    Observation {
        sampled_at_ms: value.as_ref().and(time),
        reading: Reading {
            id,
            value,
            provenance,
            unit: entity.unit,
            note,
        },
        derived_from,
    }
}

fn pipeline_readings(snapshot: &Snapshot) -> (Vec<Observation>, usize) {
    let time = (snapshot.generation > 0).then_some(snapshot.collected_at.saturating_mul(1000));
    let mut out = Vec::new();
    let missing = "The collector supplied no reading in this generation; provider error details are not available";
    macro_rules! push {
        ($id:expr, $value:expr, $reason:expr) => {
            out.push(observation($id, $value, time, $reason));
        };
    }
    push!(
        "cpu.total.idle".into(),
        snapshot.cpu.as_ref().map(|c| json!(c.total.idle)),
        missing
    );
    push!(
        "cpu.total.utilization".into(),
        snapshot.cpu_utilization().map(|v| json!(v)),
        missing
    );
    for (id, value) in [
        (
            "memory.total",
            snapshot
                .memory
                .as_ref()
                .map(|m| m.ram.total.saturating_mul(1024)),
        ),
        (
            "memory.used",
            snapshot
                .memory
                .as_ref()
                .map(|m| m.ram.used.saturating_mul(1024)),
        ),
        (
            "memory.swap.total",
            snapshot
                .memory
                .as_ref()
                .and_then(|m| m.swap.total)
                .map(|v| v.saturating_mul(1024)),
        ),
        (
            "memory.swap.used",
            snapshot
                .memory
                .as_ref()
                .and_then(|m| m.swap.used)
                .map(|v| v.saturating_mul(1024)),
        ),
    ] {
        push!(id.into(), value.map(|v| json!(v)), missing);
    }
    push!(
        "memory.utilization".into(),
        snapshot
            .memory
            .as_ref()
            .filter(|m| m.ram.total > 0)
            .map(|m| json!(m.ram_usage_percent())),
        missing
    );
    for (index, descriptor) in snapshot.gpu_static.iter().enumerate() {
        let gpu = snapshot.gpu_dynamic.get(index).and_then(Option::as_ref);
        for (suffix, value) in [
            (
                "utilization",
                gpu.and_then(|g| g.utilization).map(|v| json!(v)),
            ),
            (
                "memory.used",
                gpu.and_then(|g| g.memory.used).map(|v| json!(v)),
            ),
            (
                "memory.total",
                gpu.and_then(|g| g.memory.total).map(|v| json!(v)),
            ),
            (
                "thermal.temperature",
                gpu.and_then(|g| g.thermal.temperature).map(|v| json!(v)),
            ),
            (
                "power.draw",
                gpu.and_then(|g| g.power.draw).map(|v| json!(v)),
            ),
        ] {
            push!(format!("gpu.{}.{suffix}", descriptor.index), value, missing);
        }
    }
    for net in &snapshot.network {
        let base = format!(
            "network.{}",
            crate::ontology::resolve::id_segment(&net.name)
        );
        push!(
            format!("{base}.rx_bytes"),
            Some(json!(net.rx_bytes)),
            missing
        );
        push!(
            format!("{base}.tx_bytes"),
            Some(json!(net.tx_bytes)),
            missing
        );
        for (direction, rate) in [("rx", net.rx_rate), ("tx", net.tx_rate)] {
            let mut reading = observation(
                format!("{base}.{direction}_rate"),
                rate.map(|v| json!(v)),
                time,
                "A rate needs two valid samples; the collector has not established a rate",
            );
            if reading.reading.value.is_some() {
                reading.reading.provenance = Provenance::Derived;
                reading.derived_from = vec![format!("{base}.{direction}_bytes")];
            }
            out.push(reading);
        }
    }
    // Cached process tables are not observations of this cycle. They remain
    // available, with their original timestamp, through inspect_process.
    if snapshot.process_sampled && !snapshot.warmup {
        for process in snapshot.processes.iter().take(128) {
            for (suffix, value) in [
                ("cpu", json!(process.cpu_percent)),
                ("memory", json!(process.memory_bytes)),
                ("name", json!(process.name)),
            ] {
                out.push(observation(
                    format!("process.{}.{suffix}", process.pid),
                    Some(value),
                    time,
                    missing,
                ));
            }
        }
    }
    let omitted =
        out.len().saturating_sub(MAX_READINGS) + snapshot.processes.len().saturating_sub(128) * 3;
    out.truncate(MAX_READINGS);
    (out, omitted)
}

struct HistoryBatch {
    generation: u64,
    at_ms: u64,
    readings: Vec<Observation>,
    identities: BTreeMap<u32, Option<u64>>,
    bytes: usize,
}

struct Store {
    latest: Arc<Snapshot>,
    readings: Vec<Observation>,
    omitted: usize,
    history: VecDeque<HistoryBatch>,
    bytes: usize,
    evicted: u64,
    event_sequence: u64,
    process_at_ms: Option<u64>,
    connections_at_ms: Option<u64>,
    connection_errors: Vec<String>,
}

impl Default for Store {
    fn default() -> Self {
        Self {
            latest: Arc::new(Snapshot::default()),
            readings: Vec::new(),
            omitted: 0,
            history: VecDeque::new(),
            bytes: 0,
            evicted: 0,
            event_sequence: 0,
            process_at_ms: None,
            connections_at_ms: None,
            connection_errors: Vec::new(),
        }
    }
}

pub(super) struct ObservationService {
    source: Option<SnapshotHandle>,
    _collector: Option<Collector>,
    store: Mutex<Store>,
    events: EventManager,
    session: String,
}

impl ObservationService {
    fn shared() -> Arc<Self> {
        SHARED
            .get_or_init(|| {
                let collector = GUI_SOURCE
                    .get()
                    .filter(|source| !source.is_stopping())
                    .is_none()
                    .then(|| {
                        Collector::spawn(CollectorConfig {
                            process_every_n_ticks: 2,
                            connection_every_n_ticks: 2,
                            history_size: 2,
                            ..Default::default()
                        })
                    });
                let source = GUI_SOURCE
                    .get()
                    .filter(|source| !source.is_stopping())
                    .cloned()
                    .or_else(|| collector.as_ref().map(Collector::handle));
                let service = Arc::new(Self {
                    source,
                    _collector: collector,
                    store: Mutex::new(Store::default()),
                    events: EventManager::new(MAX_EVENTS),
                    session: format!("{}-{}", now_ms(), std::process::id()),
                });
                let weak = Arc::downgrade(&service);
                let _ = std::thread::Builder::new()
                    .name("agent-observations".into())
                    .spawn(move || {
                        while let Some(service) = weak.upgrade() {
                            service.refresh();
                            if service
                                .source
                                .as_ref()
                                .is_some_and(SnapshotHandle::is_stopping)
                            {
                                break;
                            }
                            drop(service);
                            std::thread::sleep(Duration::from_millis(200));
                        }
                    });
                service
            })
            .clone()
    }

    fn refresh(&self) {
        let Some(source) = self.source.as_ref() else {
            return;
        };
        let snapshot = source.latest();
        self.ingest(snapshot);
    }

    fn ingest(&self, snapshot: Arc<Snapshot>) {
        let mut store = self.store.lock().unwrap_or_else(|e| e.into_inner());
        if snapshot.generation <= store.latest.generation {
            return;
        }
        let (readings, omitted) = pipeline_readings(&snapshot);
        let at_ms = snapshot.collected_at.saturating_mul(1000);
        if snapshot.process_sampled && !snapshot.warmup {
            store.process_at_ms = Some(at_ms);
        }
        if snapshot.connections_sampled && !snapshot.warmup {
            store.connections_at_ms = Some(at_ms);
            if let Some(errors) = &snapshot.connection_errors {
                store.connection_errors = errors.clone();
            }
        }
        if !snapshot.warmup {
            for current in &readings {
                let previous = store
                    .readings
                    .iter()
                    .find(|p| p.reading.id == current.reading.id);
                let changed = previous
                    .is_some_and(|p| p.reading.value.is_some() != current.reading.value.is_some());
                let threshold = matches!(
                    current.reading.id.as_str(),
                    "cpu.total.utilization" | "memory.utilization"
                ) && current
                    .reading
                    .value
                    .as_ref()
                    .and_then(Value::as_f64)
                    .is_some_and(|v| v >= 90.0)
                    && !previous
                        .and_then(|p| p.reading.value.as_ref())
                        .and_then(Value::as_f64)
                        .is_some_and(|v| v >= 90.0);
                if changed || threshold {
                    store.event_sequence += 1;
                    let sequence = store.event_sequence;
                    let event = SystemEvent::new(
                        EventCategory::System,
                        if threshold {
                            EventSeverity::Warning
                        } else {
                            EventSeverity::Info
                        },
                        if threshold {
                            "threshold_crossed"
                        } else {
                            "reading_availability_changed"
                        },
                        format!("Observation changed: {}", current.reading.id),
                        &current.reading.id,
                    )
                    .with_metadata("sequence", sequence)
                    .with_metadata("generation", snapshot.generation)
                    .with_metadata("provenance", current.reading.provenance)
                    .with_metadata("reading", &current.reading);
                    self.events.emit(event);
                }
            }
            let identities: BTreeMap<_, _> = snapshot
                .processes
                .iter()
                .take(128)
                .map(|p| (p.pid, p.start_time))
                .collect();
            let bytes =
                serde_json::to_vec(&readings).map_or(0, |v| v.len()) + identities.len() * 32 + 64;
            store.bytes += bytes;
            store.history.push_back(HistoryBatch {
                generation: snapshot.generation,
                at_ms,
                readings: readings.clone(),
                identities,
                bytes,
            });
            while store.bytes > HISTORY_BYTES || store.history.len() > HISTORY_BATCHES {
                if let Some(old) = store.history.pop_front() {
                    store.bytes -= old.bytes;
                    store.evicted += 1;
                }
            }
        }
        store.latest = snapshot;
        store.readings = readings;
        store.omitted = omitted;
    }

    fn wait(&self, milliseconds: u64) {
        let deadline = Instant::now() + Duration::from_millis(milliseconds);
        loop {
            self.refresh();
            let store = self.store.lock().unwrap_or_else(|e| e.into_inner());
            let ready = store.latest.generation > 0 && !store.latest.warmup;
            drop(store);
            if ready || Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

fn default_ids() -> Vec<String> {
    ["cpu.total.utilization", "memory.total", "memory.used"]
        .into_iter()
        .map(str::to_string)
        .collect()
}
fn default_age() -> u64 {
    5000
}
fn default_points() -> u64 {
    120
}
fn default_limit() -> u64 {
    100
}
fn default_window() -> u64 {
    30
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SnapshotParams {
    #[serde(default)]
    include_collected_ids: bool,
    #[serde(default = "default_ids")]
    ids: Vec<String>,
    #[serde(default = "default_age")]
    max_age_ms: u64,
    #[serde(default)]
    wait_ms: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HealthParams {
    #[serde(default = "default_age")]
    max_age_ms: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoryParams {
    ids: Vec<String>,
    start_ms: u64,
    end_ms: u64,
    #[serde(default = "default_points")]
    max_points: u64,
    #[serde(default)]
    aggregation: Aggregation,
}
#[derive(Clone, Copy, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum Aggregation {
    #[default]
    Last,
    Min,
    Max,
    Avg,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EventParams {
    #[serde(default)]
    cursor: Option<String>,
    #[serde(default = "default_limit")]
    limit: u64,
    #[serde(default)]
    filter: EventFilter,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProcessParams {
    pid: u32,
    #[serde(default)]
    expected_start_time: Option<u64>,
    #[serde(default = "default_window")]
    window_secs: u64,
    #[serde(default = "default_limit")]
    connection_limit: u64,
}

impl AiDataApi {
    pub(super) fn tool_list_connections(&mut self, params: Value) -> Result<Value> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Params {
            #[serde(default)]
            pid: Option<u32>,
            #[serde(default)]
            local_port: Option<u16>,
            #[serde(default)]
            remote_ip: Option<std::net::IpAddr>,
            #[serde(default)]
            listening_only: bool,
            #[serde(default = "default_age")]
            max_age_ms: u64,
            #[serde(default = "default_limit")]
            limit: u64,
        }
        let p: Params = parse(params)?;
        bounded(p.limit, 1, 100, "limit")?;
        bounded(p.max_age_ms, 1, 60000, "max_age_ms")?;
        if p.pid == Some(0) {
            return Err(bad("PID must be positive"));
        }
        let service = self.observation_service();
        service.refresh();
        let store = service.store.lock().unwrap_or_else(|e| e.into_inner());
        let age = store.connections_at_ms.map(|t| now_ms().saturating_sub(t));
        if store.latest.connections.is_empty() && !store.connection_errors.is_empty() {
            return Ok(
                json!({"provenance":"unavailable","reason":"Connection enumeration was incomplete and supplied no rows",
                "provider_errors":store.connection_errors,"sampled_at_ms":store.connections_at_ms}),
            );
        }
        if age.is_none_or(|age| age > p.max_age_ms) {
            return Ok(
                json!({"provenance":"unavailable","reason":"No connection table within the requested freshness bound","sampled_at_ms":store.connections_at_ms,"age_ms":age}),
            );
        }
        let matching: Vec<_> = store
            .latest
            .connections
            .iter()
            .filter(|c| p.pid.is_none_or(|pid| c.pid == Some(pid)))
            .filter(|c| p.local_port.is_none_or(|port| c.local_port == port))
            .filter(|c| p.remote_ip.is_none_or(|ip| c.remote_ip == Some(ip)))
            .filter(|c| !p.listening_only || c.state == crate::connections::ConnectionState::Listen)
            .collect();
        let rows:Vec<_>=matching.iter().take(p.limit as usize).map(|c| {
            let identity=store.latest.processes.iter().find(|process|Some(process.pid)==c.pid)
                .filter(|process|connection_identity_verified(process.start_time,store.connections_at_ms));
            json!({"protocol":c.protocol,"local_address":c.local_address,"remote_address":c.remote_address,
                "state":c.state,"pid":c.pid,"process_name":identity.map(|process|&process.name),
                "process_start_time":identity.and_then(|process|process.start_time),"identity_verified":identity.is_some()})
        }).collect();
        Ok(
            json!({"generation":store.latest.generation,"provenance":"measured","sampled_at_ms":store.connections_at_ms,
            "age_ms":age,"connections":rows,"matched":matching.len(),"truncated":matching.len()>rows.len(),
            "partial":!store.connection_errors.is_empty(),"provider_errors":store.connection_errors,
            "note":"Socket PIDs came from the OS; process names are withheld unless the last process table verifies identity. Partial enumeration is not proof of absence."}),
        )
    }

    fn observation_service(&mut self) -> Arc<ObservationService> {
        self.observations
            .get_or_insert_with(ObservationService::shared)
            .clone()
    }

    pub(super) fn tool_observation_snapshot(&mut self, params: Value) -> Result<Value> {
        let p: SnapshotParams = parse(params)?;
        ids_valid(&p.ids, 128)?;
        bounded(p.max_age_ms, 1, 60_000, "max_age_ms")?;
        bounded(p.wait_ms, 0, 1000, "wait_ms")?;
        let service = self.observation_service();
        service.wait(p.wait_ms);
        let store = service.store.lock().unwrap_or_else(|e| e.into_inner());
        let age = now_ms().saturating_sub(store.latest.collected_at.saturating_mul(1000));
        let readings: Vec<_> = p
            .ids
            .iter()
            .map(|id| {
                if store.latest.generation == 0 || age > p.max_age_ms {
                    return observation(
                        id.clone(),
                        None,
                        None,
                        "No snapshot within the requested freshness bound",
                    );
                }
                store
                    .readings
                    .iter()
                    .find(|r| r.reading.id == *id)
                    .cloned()
                    .unwrap_or_else(|| {
                        observation(id.clone(), None, None,
                    "This entity was not sampled by the bounded collector in this generation")
                    })
            })
            .collect();
        Ok(
            json!({"schema_version": ONTOLOGY_VERSION, "session": service.session,
            "generation": store.latest.generation, "warmup": store.latest.warmup,
            "snapshot_at_ms": (store.latest.generation > 0).then_some(store.latest.collected_at.saturating_mul(1000)),
            "timestamp_precision_ms": 1000, "age_ms": (store.latest.generation > 0).then_some(age),
            "readings": readings, "collector_readings_omitted": store.omitted,
            "collected_ids": p.include_collected_ids.then(||store.readings.iter().take(128).map(|r|&r.reading.id).collect::<Vec<_>>()),
            "collected_ids_truncated": p.include_collected_ids && store.readings.len()>128 }),
        )
    }

    pub(super) fn tool_collector_health(&mut self, params: Value) -> Result<Value> {
        let p: HealthParams = parse(params)?;
        bounded(p.max_age_ms, 1, 60_000, "max_age_ms")?;
        let service = self.observation_service();
        service.refresh();
        let store = service.store.lock().unwrap_or_else(|e| e.into_inner());
        let ready = store.latest.generation > 0;
        let age =
            ready.then(|| now_ms().saturating_sub(store.latest.collected_at.saturating_mul(1000)));
        let stopped = service
            .source
            .as_ref()
            .is_some_and(SnapshotHandle::is_stopping);
        let unavailable: Vec<_> = store
            .readings
            .iter()
            .filter(|r| r.reading.provenance == Provenance::Unavailable)
            .collect();
        Ok(
            json!({"session": service.session, "generation": store.latest.generation,
            "status": if stopped {"stopping"} else if !ready {"initializing"} else if age.is_some_and(|a| a > p.max_age_ms) {"stale"} else if store.latest.warmup {"warming_up"} else {"active"},
            "age_ms": age, "timestamp_precision_ms": 1000,
            "collection_us": ready.then_some(store.latest.collect_us), "stage_timings_us": ready.then_some(store.latest.timings),
            "process_sampled_this_tick": store.latest.process_sampled,
            "connections_sampled_this_tick": store.latest.connections_sampled,
            "connection_provider_errors": store.connection_errors,
            "unavailable_readings": unavailable, "provider_error_details": "Exact provider exceptions are not carried by the pipeline",
            "history": {"batches": store.history.len(), "serialized_bytes": store.bytes,
                "byte_limit": HISTORY_BYTES, "batch_limit": HISTORY_BATCHES, "evicted_batches": store.evicted},
            "collector_readings_omitted": store.omitted }),
        )
    }

    pub(super) fn tool_metric_history(&mut self, params: Value) -> Result<Value> {
        let p: HistoryParams = parse(params)?;
        ids_valid(&p.ids, 16)?;
        bounded(p.max_points, 1, 600, "max_points")?;
        if p.start_ms > p.end_ms || p.end_ms.saturating_sub(p.start_ms) > 3_600_000 {
            return Err(bad("History range must be ordered and at most one hour"));
        }
        let service = self.observation_service();
        service.refresh();
        let store = service.store.lock().unwrap_or_else(|e| e.into_inner());
        let series: Vec<_> = p
            .ids
            .iter()
            .map(|id| history_series(&store, id, &p, None))
            .collect();
        Ok(json!({"session": service.session, "series": series,
            "retention_start_ms": store.history.front().map(|b| b.at_ms),
            "retention_end_ms": store.history.back().map(|b| b.at_ms),
            "retention_truncated": store.evicted > 0 && store.history.front().is_some_and(|b| p.start_ms < b.at_ms),
            "collection_scope": "Only readings retained by this session's bounded collector; no pre-session history"}))
    }

    pub(super) fn tool_observation_events(&mut self, params: Value) -> Result<Value> {
        if let Some(filter) = params.get("filter").and_then(Value::as_object) {
            if filter.keys().any(|key| {
                !matches!(
                    key.as_str(),
                    "categories" | "min_severity" | "source_pattern" | "event_types"
                )
            }) {
                return Err(bad("Unrecognized event filter field"));
            }
        }
        let p: EventParams = parse(params)?;
        if p.filter
            .categories
            .as_ref()
            .is_some_and(|values| values.len() > 16)
            || p.filter
                .source_pattern
                .as_ref()
                .is_some_and(|value| value.len() > 128)
            || p.filter
                .event_types
                .as_ref()
                .is_some_and(|values| values.len() > 16 || values.iter().any(|v| v.len() > 64))
        {
            return Err(bad("Event filters exceed the declared size limits"));
        }
        bounded(p.limit, 1, 100, "limit")?;
        let service = self.observation_service();
        service.refresh();
        let (sequence, mut retained) = {
            let store = service.store.lock().unwrap_or_else(|e| e.into_inner());
            (
                store.event_sequence,
                service.events.get_events(None, Some(MAX_EVENTS)),
            )
        };
        let after = if let Some(cursor) = p.cursor {
            if cursor.len() > 80 {
                return Err(bad("Invalid event cursor"));
            }
            let (session, number) = cursor
                .split_once(':')
                .ok_or_else(|| bad("Invalid event cursor"))?;
            if session != service.session {
                return Err(bad("Event cursor belongs to a different collector session"));
            }
            let number = number
                .parse::<u64>()
                .map_err(|_| bad("Invalid event cursor sequence"))?;
            if number > sequence {
                return Err(bad("Event cursor is ahead of the collector"));
            }
            number
        } else {
            0
        };
        retained.sort_by_key(|event| {
            event
                .metadata
                .get("sequence")
                .and_then(Value::as_u64)
                .unwrap_or(0)
        });
        let earliest = retained
            .first()
            .and_then(|event| event.metadata.get("sequence"))
            .and_then(Value::as_u64);
        let matching: Vec<_> = retained
            .into_iter()
            .filter(|event| {
                event
                    .metadata
                    .get("sequence")
                    .and_then(Value::as_u64)
                    .is_some_and(|n| n > after)
                    && p.filter.matches(event)
            })
            .collect();
        let more = matching.len() > p.limit as usize;
        let events: Vec<_> = matching.into_iter().take(p.limit as usize).collect();
        let next = if more {
            events
                .last()
                .and_then(|e| e.metadata.get("sequence"))
                .and_then(Value::as_u64)
                .unwrap_or(after)
        } else {
            sequence
        };
        Ok(json!({"session": service.session, "events": events,
            "next_cursor": format!("{}:{next}", service.session), "has_more": more,
            "events_lost": earliest.is_some_and(|first| first > after.saturating_add(1)),
            "retention_limit": MAX_EVENTS,
            "scope": "Observed threshold crossings and reading availability changes; not operating-system event logs"}))
    }

    pub(super) fn tool_inspect_process(&mut self, params: Value) -> Result<Value> {
        let p: ProcessParams = parse(params)?;
        bounded(p.pid as u64, 1, u32::MAX as u64, "pid")?;
        bounded(p.window_secs, 1, 30, "window_secs")?;
        bounded(p.connection_limit, 1, 100, "connection_limit")?;
        let service = self.observation_service();
        service.refresh();
        let store = service.store.lock().unwrap_or_else(|e| e.into_inner());
        let Some(process) = store
            .latest
            .processes
            .iter()
            .find(|process| process.pid == p.pid)
        else {
            return Ok(
                json!({"generation": store.latest.generation, "pid": p.pid, "process": null,
                "provenance": "unavailable", "reason": "PID was not present in the collector's last process table"}),
            );
        };
        if store.process_at_ms.is_none() {
            return Ok(
                json!({"pid":p.pid,"process":null,"provenance":"unavailable",
                "reason":"This session has not observed a process-table refresh; cached values are withheld"}),
            );
        }
        if let Some(expected) = p.expected_start_time {
            if process.start_time != Some(expected) {
                return Err(bad("Process identity could not be verified: start time differs or was not reported"));
            }
        }
        let matches: Vec<_> = store
            .latest
            .connections
            .iter()
            .filter(|c| c.pid == Some(p.pid))
            .collect();
        let connections_identity_verified =
            connection_identity_verified(process.start_time, store.connections_at_ms);
        let connections: Vec<_> = matches
            .iter()
            .filter(|_| connections_identity_verified)
            .take(p.connection_limit as usize)
            .collect();
        let history_params = HistoryParams {
            ids: Vec::new(),
            start_ms: now_ms().saturating_sub(p.window_secs * 1000),
            end_ms: now_ms(),
            max_points: 30,
            aggregation: Aggregation::Last,
        };
        let history: Vec<_> = ["cpu", "memory"]
            .into_iter()
            .map(|suffix| {
                history_series(
                    &store,
                    &format!("process.{}.{suffix}", p.pid),
                    &history_params,
                    Some((p.pid, process.start_time)),
                )
            })
            .collect();
        Ok(json!({"generation": store.latest.generation,
            "process": {"pid": process.pid, "name": process.name, "start_time": process.start_time,
                "sampled_at_ms": store.process_at_ms, "cpu_percent": process.cpu_percent,
                "memory_bytes": process.memory_bytes, "gpu_usage_percent": process.gpu_usage_percent,
                "gpu_memory_bytes": process.total_gpu_memory_bytes, "gpu_indices": process.gpu_indices},
            "identity_verified": process.start_time.is_some(), "history": history,
            "connections": connections, "connections_sampled_at_ms": store.connections_at_ms,
            "connections_identity_verified": connections_identity_verified,
            "connections_partial":!store.connection_errors.is_empty(),"connection_provider_errors":store.connection_errors,
            "connections_note": (!connections_identity_verified).then_some("Connection table could not be tied to this process identity; matching PIDs alone are insufficient"),
            "connections_truncated": matches.len() > connections.len(),
            "note": "Last-observed tables retain their collection timestamps; missing GPU values are not zero",
            "history_identity_note": process.start_time.is_none().then_some("Process start time was not reported; history is withheld to avoid joining reused PIDs")}))
    }
}

fn connection_identity_verified(start: Option<u64>, sampled: Option<u64>) -> bool {
    // Both source timestamps have one-second precision. A table from the same
    // second could precede process creation, so it cannot establish identity.
    start
        .zip(sampled)
        .is_some_and(|(start, sampled)| start.saturating_add(1).saturating_mul(1000) <= sampled)
}

fn history_series(
    store: &Store,
    id: &str,
    params: &HistoryParams,
    identity: Option<(u32, Option<u64>)>,
) -> Value {
    let batches: Vec<_> = store
        .history
        .iter()
        .filter(|b| b.at_ms >= params.start_ms && b.at_ms <= params.end_ms)
        .collect();
    let bucket_size = batches.len().div_ceil(params.max_points as usize).max(1);
    let mut points = Vec::new();
    for bucket in batches.chunks(bucket_size) {
        let mut valid = Vec::new();
        let mut gaps = 0;
        for batch in bucket {
            let verified = identity.is_none_or(|(pid, start)| {
                start.is_some() && batch.identities.get(&pid) == Some(&start)
            });
            let reading = verified
                .then(|| batch.readings.iter().find(|r| r.reading.id == id))
                .flatten();
            if let Some((reading, value)) = reading.and_then(|r| {
                r.reading
                    .value
                    .as_ref()
                    .and_then(Value::as_f64)
                    .map(|v| (r, v))
            }) {
                valid.push((reading, value));
            } else {
                gaps += 1;
            }
        }
        let last = bucket.last().expect("nonempty history bucket");
        let value = match params.aggregation {
            Aggregation::Last => {
                let verified = identity.is_none_or(|(pid, start)| {
                    start.is_some() && last.identities.get(&pid) == Some(&start)
                });
                verified
                    .then(|| last.readings.iter().find(|r| r.reading.id == id))
                    .flatten()
                    .and_then(|r| r.reading.value.as_ref())
                    .and_then(Value::as_f64)
            }
            Aggregation::Min => valid.iter().map(|(_, v)| *v).reduce(f64::min),
            Aggregation::Max => valid.iter().map(|(_, v)| *v).reduce(f64::max),
            Aggregation::Avg => (!valid.is_empty())
                .then(|| valid.iter().map(|(_, v)| v).sum::<f64>() / valid.len() as f64),
        };
        let provenance = if value.is_none() {
            Provenance::Unavailable
        } else if bucket.len() > 1 || !matches!(params.aggregation, Aggregation::Last) {
            Provenance::Derived
        } else {
            valid[0].0.reading.provenance
        };
        let mut point = json!({"timestamp_ms": last.at_ms, "generation": last.generation,
            "provenance": provenance, "samples": valid.len(), "gaps": gaps,
            "derived_from": if provenance == Provenance::Derived { vec![id] } else { Vec::new() },
            "bucket_start_ms": bucket[0].at_ms,
            "note": value.is_none().then_some("No numeric observation for this bucket; the gap is not zero")});
        if let Some(value) = value {
            point["value"] = json!(value);
        }
        points.push(point);
    }
    json!({"id": id, "unit": ontology().template_for(id).and_then(|e| e.unit),
        "aggregation": params.aggregation, "points": points,
        "downsampled": bucket_size > 1, "timestamp_precision_ms": 1000})
}

pub(super) fn definitions() -> Vec<ToolDefinition> {
    let ids = json!({"type":"array","minItems":1,"maxItems":128,"items":{"type":"string","maxLength":128}});
    let history_ids = json!({"type":"array","minItems":1,"maxItems":16,"items":{"type":"string","maxLength":128}});
    let definitions = [
        ("get_observation_snapshot", "Read concrete ontology IDs from one bounded collector generation, with provenance, freshness, and unavailable reasons. No independent hardware sampling.",
            json!({"ids":ids,"include_collected_ids":{"type":"boolean","default":false,"description":"Return up to 128 concrete IDs from this generation to expand schema templates; truncation is explicit."},"max_age_ms":{"type":"integer","minimum":1,"maximum":60000,"default":5000},"wait_ms":{"type":"integer","minimum":0,"maximum":1000,"default":0}}), vec![]),
        ("get_collector_health", "Report collector generation, freshness, stage timings, unavailable readings, and bounded history retention. Exact provider exceptions are not available from this pipeline.",
            json!({"max_age_ms":{"type":"integer","minimum":1,"maximum":60000,"default":5000}}), vec![]),
        ("query_metric_history", "Query numeric readings retained since this collector session started. Maximum one-hour range, 16 IDs and 600 points per series. Missing readings remain explicit gaps.",
            json!({"ids":history_ids,"start_ms":{"type":"integer","minimum":0},"end_ms":{"type":"integer","minimum":0},"max_points":{"type":"integer","minimum":1,"maximum":600,"default":120},"aggregation":{"type":"string","enum":["last","min","max","avg"],"default":"last"}}), vec!["ids","start_ms","end_ms"]),
        ("query_events", "Page observed threshold and reading-availability events using a session-bound cursor. Does not read operating-system logs. Lost retained events are announced.",
            json!({"cursor":{"type":"string","maxLength":80},"limit":{"type":"integer","minimum":1,"maximum":100,"default":100},"filter":{"type":"object","additionalProperties":false,"properties":{"categories":{"type":"array","maxItems":16,"items":{"type":"string"}},"min_severity":{"type":"string","enum":["info","warning","error","critical"]},"source_pattern":{"type":"string","maxLength":128},"event_types":{"type":"array","maxItems":16,"items":{"type":"string","maxLength":64}}}}}), vec![]),
        ("inspect_process", "Inspect a last-observed PID, its GPU attribution, bounded connections, and up to 30 seconds of retained CPU/memory history. Optional start-time verification prevents joining reused PIDs.",
            json!({"pid":{"type":"integer","minimum":1,"maximum":u32::MAX},"expected_start_time":{"type":"integer","minimum":0},"window_secs":{"type":"integer","minimum":1,"maximum":30,"default":30},"connection_limit":{"type":"integer","minimum":1,"maximum":100,"default":100}}), vec!["pid"]),
        ("check_endpoint", "Probe one explicit HTTP(S) endpoint through DNS, TCP, TLS and HTTP stages, with one overall deadline, bounded headers and no redirects or response-body capture. HTTP(S) support requires remote-backends.",
            json!({"url":{"type":"string","maxLength":2048},"timeout_ms":{"type":"integer","minimum":100,"maximum":10000,"default":3000},"method":{"type":"string","enum":["HEAD","GET"],"default":"HEAD"}}), vec!["url"]),
    ];
    definitions.into_iter().map(|(name, description, properties, required)| ToolDefinition {
        name: name.into(), description: description.into(),
        parameters: json!({"type":"object","properties":properties,"required":required,"additionalProperties":false}),
        category: ToolCategory::System, example: None,
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::cpu::{CpuStats, CpuTotal};

    #[test]
    fn observation_utc_preserves_sample_time_and_absence() {
        let sample = observation(
            "memory.used".into(),
            Some(json!(42)),
            Some(1790805591000),
            "missing",
        );
        let value = serde_json::to_value(sample).unwrap();
        assert_eq!(value["sampled_at_ms"], 1790805591000u64);
        assert_eq!(value["sampled_at_utc"], "2026-09-30T21:59:51.000Z");
        let missing = observation("memory.used".into(), None, Some(1790805591000), "missing");
        let value = serde_json::to_value(missing).unwrap();
        assert!(value["sampled_at_utc"].is_null());
        assert!(value.get("value").is_none());
    }

    fn fixture() -> (AiDataApi, Arc<ObservationService>) {
        let service = Arc::new(ObservationService {
            source: None,
            _collector: None,
            store: Mutex::new(Store::default()),
            events: EventManager::new(MAX_EVENTS),
            session: "fixture".into(),
        });
        let mut api = AiDataApi::with_components(None, None, None);
        api.observations = Some(service.clone());
        (api, service)
    }

    fn sample(generation: u64, at: u64, idle: Option<f32>) -> Arc<Snapshot> {
        Arc::new(Snapshot {
            generation,
            collected_at: at,
            warmup: false,
            cpu: idle.map(|idle| CpuStats {
                cores: Vec::new(),
                total: CpuTotal {
                    user: 100.0 - idle,
                    nice: 0.0,
                    system: 0.0,
                    idle,
                },
            }),
            ..Default::default()
        })
    }

    #[test]
    fn snapshots_preserve_generation_and_withhold_stale_or_missing_values() {
        let (mut api, service) = fixture();
        service.ingest(sample(7, now_ms() / 1000, Some(70.0)));
        let result = api
            .tool_observation_snapshot(json!({"ids":["cpu.total.utilization", "memory.used"]}))
            .unwrap();
        assert_eq!(result["generation"], 7);
        assert_eq!(result["readings"][0]["value"], 30.0);
        assert_eq!(result["readings"][1]["provenance"], "unavailable");
        assert!(result["readings"][1].get("value").is_none());
        service.ingest(sample(8, now_ms() / 1000 - 60, Some(50.0)));
        let result = api
            .tool_observation_snapshot(json!({"ids":["cpu.total.utilization"]}))
            .unwrap();
        assert!(result["readings"][0].get("value").is_none());
        assert!(api
            .tool_observation_snapshot(json!({"ids":["gpu.{n}.name"]}))
            .is_err());
        assert!(api
            .tool_observation_snapshot(json!({"ids":["typo"]}))
            .is_err());
    }
    #[test]
    fn connection_queries_preserve_filters_and_failed_refreshes() {
        use crate::connections::{ConnectionInfo, ConnectionState, Protocol};
        let (mut api, service) = fixture();
        let mut snapshot = sample(1, now_ms() / 1000, Some(70.0)).as_ref().clone();
        snapshot.connections_sampled = true;
        snapshot.connection_errors = Some(vec![]);
        snapshot.connections = vec![ConnectionInfo {
            protocol: Protocol::Tcp,
            local_address: "127.0.0.1:11434".into(),
            local_ip: "127.0.0.1".parse().unwrap(),
            local_port: 11434,
            remote_address: None,
            remote_ip: None,
            remote_port: None,
            state: ConnectionState::Listen,
            pid: Some(42),
            process_name: Some("unverified".into()),
        }];
        service.ingest(Arc::new(snapshot.clone()));
        let result = api
            .tool_list_connections(json!({"local_port":11434,"listening_only":true}))
            .unwrap();
        assert_eq!(result["matched"], 1);
        assert_eq!(result["partial"], false);
        assert!(result["connections"][0]["process_name"].is_null());
        assert_eq!(
            api.tool_list_connections(json!({"local_port":1})).unwrap()["matched"],
            0
        );
        snapshot.generation = 2;
        snapshot.connections.clear();
        snapshot.connection_errors = Some(vec!["TCP enumeration failed".into()]);
        service.ingest(Arc::new(snapshot.clone()));
        assert_eq!(
            api.tool_list_connections(json!({})).unwrap()["provenance"],
            "unavailable"
        );
        snapshot.generation = 3;
        snapshot.connections_sampled = false;
        snapshot.connection_errors = None;
        service.ingest(Arc::new(snapshot));
        assert_eq!(
            api.tool_list_connections(json!({})).unwrap()["provenance"],
            "unavailable"
        );
    }

    #[test]
    fn history_keeps_gaps_and_aggregates_only_observations() {
        let (_, service) = fixture();
        service.ingest(sample(1, 10, Some(80.0)));
        service.ingest(sample(2, 11, None));
        service.ingest(sample(3, 12, Some(60.0)));
        let store = service.store.lock().unwrap();
        let mut params = HistoryParams {
            ids: vec!["cpu.total.utilization".into()],
            start_ms: 0,
            end_ms: 20_000,
            max_points: 1,
            aggregation: Aggregation::Avg,
        };
        let series = history_series(&store, &params.ids[0], &params, None);
        assert_eq!(series["points"][0]["value"], 30.0);
        assert_eq!(series["points"][0]["gaps"], 1);
        assert_eq!(series["points"][0]["provenance"], "derived");
        params.aggregation = Aggregation::Last;
        assert_eq!(
            history_series(&store, &params.ids[0], &params, None)["points"][0]["value"],
            40.0
        );
        params.max_points = 3;
        let series = history_series(&store, &params.ids[0], &params, None);
        assert!(series["points"][1].get("value").is_none());
    }

    #[test]
    fn history_retention_and_cursor_sessions_are_bounded() {
        let (mut api, service) = fixture();
        for generation in 1..=150 {
            service.ingest(sample(generation, generation, Some(50.0)));
        }
        let store = service.store.lock().unwrap();
        assert_eq!(store.history.len(), HISTORY_BATCHES);
        assert!(store.bytes <= HISTORY_BYTES);
        assert_eq!(store.evicted, 30);
        drop(store);
        assert!(api
            .tool_observation_events(json!({"cursor":"other:1"}))
            .is_err());
        assert!(api
            .tool_observation_events(json!({"filter":{"unexpected":true}}))
            .is_err());
    }

    #[test]
    fn invalid_measurements_and_oversized_text_are_unavailable() {
        for value in [json!(-1), json!(101)] {
            let reading = observation(
                "cpu.total.utilization".into(),
                Some(value),
                Some(1),
                "missing",
            );
            assert_eq!(reading.reading.provenance, Provenance::Unavailable);
            assert!(reading.reading.value.is_none());
        }
        assert!(observation(
            "process.1.name".into(),
            Some(json!("x".repeat(1025))),
            Some(1),
            "missing"
        )
        .reading
        .value
        .is_none());
    }

    #[test]
    fn event_pagination_announces_eviction_and_advances_cursors() {
        let (mut api, service) = fixture();
        for generation in 1..=300 {
            service.ingest(sample(
                generation,
                generation,
                (generation % 2 == 0).then_some(50.0),
            ));
        }
        let first = api.tool_observation_events(json!({"limit":2})).unwrap();
        assert_eq!(first["events"].as_array().unwrap().len(), 2);
        assert_eq!(first["events_lost"], true);
        assert_eq!(first["has_more"], true);
        let second = api
            .tool_observation_events(json!({"limit":2,"cursor":first["next_cursor"]}))
            .unwrap();
        assert_ne!(
            first["events"][0]["metadata"]["sequence"],
            second["events"][0]["metadata"]["sequence"]
        );
        assert_eq!(second["events_lost"], false);
    }

    #[test]
    fn process_history_does_not_join_reused_or_unverified_pids() {
        assert!(!connection_identity_verified(Some(2), Some(2000)));
        assert!(!connection_identity_verified(None, Some(3000)));
        assert!(connection_identity_verified(Some(2), Some(3000)));
        let mut store = Store::default();
        for (generation, start, value) in [(1, Some(1), 10), (2, Some(2), 20), (3, None, 30)] {
            store.history.push_back(HistoryBatch {
                generation,
                at_ms: generation * 1000,
                readings: vec![observation(
                    "process.42.memory".into(),
                    Some(json!(value)),
                    Some(generation * 1000),
                    "missing",
                )],
                identities: BTreeMap::from([(42, start)]),
                bytes: 0,
            });
        }
        let params = HistoryParams {
            ids: vec![],
            start_ms: 0,
            end_ms: 4000,
            max_points: 3,
            aggregation: Aggregation::Last,
        };
        let series = history_series(&store, "process.42.memory", &params, Some((42, Some(2))));
        assert!(series["points"][0].get("value").is_none());
        assert_eq!(series["points"][1]["value"], 20.0);
        assert!(series["points"][2].get("value").is_none());
        let series = history_series(&store, "process.42.memory", &params, Some((42, None)));
        assert!(series["points"]
            .as_array()
            .unwrap()
            .iter()
            .all(|point| point.get("value").is_none()));
    }
}
