//! HTTP Server for IronMonitor REST API
//!
//! Provides a lightweight HTTP/1.1 server built on tokio that exposes monitoring
//! data over JSON endpoints, Prometheus metrics and health checks.
//!
//! Two families of routes live here:
//!
//! - `/api/v1/describe`, `/api/v1/snapshot`, `/api/v1/get/<id>` and `/api/v1/health`
//!   serve the ontology. Their bodies are the CLI's own JSON, and they answer from a
//!   background sampler (see the `sampler` module) rather than resolving per request.
//! - Everything else is the older observability API and the Prometheus exporter.
//!
//! Security defaults: no CORS headers, a bearer key that is checked on every route
//! except `/api/v1/health` when one is configured, and a Host check that rejects DNS
//! rebinding when the server is bound to loopback.
//!
//! # Examples
//!
//! ```no_run
//! use ironmonitor::http_server::{HttpServer, HttpServerConfig};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = HttpServerConfig {
//!         bind_address: "127.0.0.1".into(),
//!         port: 8080,
//!         ..Default::default()
//!     };
//!     let server = HttpServer::new(config)?;
//!     println!("Listening on http://127.0.0.1:8080");
//!     server.run().await?;
//!     Ok(())
//! }
//! ```

#[cfg(feature = "cli")]
mod sampler;
#[cfg(feature = "cli")]
mod v1;

use crate::observability::permissions::ApiKey;
use crate::observability::{
    ApiConfig, HttpRequest as ObsRequest, MetricCollector, ObservabilityApi, RequestHandler,
    ServerConfig,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::sync::Arc;
#[cfg(feature = "cli")]
use v1::Reply;

/// Connections served at once. One beyond this is dropped without a response.
#[cfg(feature = "cli")]
const MAX_CONNECTIONS: usize = 64;

/// Bytes of request line and headers accepted. A longer head is refused with 431.
#[cfg(feature = "cli")]
const MAX_HEAD_BYTES: usize = 16 * 1024;

/// HTTP server configuration
#[derive(Clone, Serialize, Deserialize)]
pub struct HttpServerConfig {
    /// Bind address (default: "127.0.0.1")
    pub bind_address: String,
    /// Port (default: 8080). Zero asks the OS for a free one.
    pub port: u16,
    /// Send CORS headers to the origins in `cors_origins` (default: false).
    pub cors_enabled: bool,
    /// Origins allowed to read responses cross-origin, matched exactly (default:
    /// none). A wildcard is refused by [`HttpServer::new`].
    pub cors_origins: Vec<String>,
    /// Maximum request body size in bytes (default: 1MB)
    pub max_body_size: usize,
    /// Request timeout in seconds, covering reading the request and writing the
    /// response (default: 30)
    pub request_timeout_secs: u64,
    /// Whether to log requests to stderr (default: true)
    pub request_logging: bool,
    /// Bearer key required on every route except `/api/v1/health` (None = no key).
    ///
    /// Never serialised, and redacted from `Debug`.
    #[serde(skip_serializing)]
    pub api_key: Option<String>,
    /// Metric collection interval in seconds (default: 5)
    pub metric_interval_secs: u64,
}

impl fmt::Debug for HttpServerConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HttpServerConfig")
            .field("bind_address", &self.bind_address)
            .field("port", &self.port)
            .field("cors_enabled", &self.cors_enabled)
            .field("cors_origins", &self.cors_origins)
            .field("max_body_size", &self.max_body_size)
            .field("request_timeout_secs", &self.request_timeout_secs)
            .field("request_logging", &self.request_logging)
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .field("metric_interval_secs", &self.metric_interval_secs)
            .finish()
    }
}

impl Default for HttpServerConfig {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1".into(),
            port: 8080,
            cors_enabled: false,
            cors_origins: Vec::new(),
            max_body_size: 1_048_576,
            request_timeout_secs: 30,
            request_logging: true,
            api_key: None,
            metric_interval_secs: 5,
        }
    }
}

/// Check that a CORS origin is an exact `scheme://host[:port]`.
///
/// A wildcard is refused on purpose: `*` would let any web page read the telemetry.
pub fn validate_cors_origin(origin: &str) -> Result<(), String> {
    if origin.trim() == "*" {
        return Err(
            "refusing `*` as a CORS origin: list each origin exactly, for example \
             --cors-origin http://localhost:5173"
                .into(),
        );
    }
    let well_formed = origin
        .split_once("://")
        .map(|(scheme, rest)| {
            (scheme == "http" || scheme == "https")
                && !rest.is_empty()
                && rest
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '[' | ']'))
        })
        .unwrap_or(false);
    if well_formed {
        Ok(())
    } else {
        Err(format!(
            "CORS origin {origin:?} must be http://host[:port] or https://host[:port], \
             with no path, query or wildcard"
        ))
    }
}

/// Whether a bind address names only this machine.
pub fn is_loopback_address(bind: &str) -> bool {
    let host = bind.trim_start_matches('[').trim_end_matches(']');
    host.eq_ignore_ascii_case("localhost")
        || host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

/// A listening socket that has been bound but not yet served.
///
/// Splitting bind from serve lets the caller learn the real port (for `--port 0`)
/// before it starts answering, and fail on a taken port before printing anything
/// that claims the server is up.
pub struct BoundServer {
    #[cfg(feature = "cli")]
    listener: std::net::TcpListener,
    local: SocketAddr,
}

impl BoundServer {
    /// The address actually bound, with the port the OS chose if one was asked for.
    pub fn local_addr(&self) -> SocketAddr {
        self.local
    }
}

/// HTTP server that exposes IronMonitor data
#[allow(dead_code)]
pub struct HttpServer {
    config: HttpServerConfig,
    handler: Arc<RequestHandler>,
    metric_collector: Arc<MetricCollector>,
    #[cfg(feature = "cli")]
    sampler: sampler::Sampler,
    loopback: bool,
}

impl HttpServer {
    /// Create a new HTTP server with the given configuration
    pub fn new(config: HttpServerConfig) -> crate::Result<Self> {
        // Build the schema before the first request arrives, so that request does not
        // pay for it. Nothing here touches hardware.
        let _ = crate::ontology::resolve::ontology();

        if config.cors_enabled {
            for origin in &config.cors_origins {
                validate_cors_origin(origin).map_err(crate::IronError::Other)?;
            }
        }
        if let Some(key) = &config.api_key {
            // An empty key would be a server that accepts an empty bearer token. Refuse
            // rather than quietly running open.
            if key.trim().is_empty() {
                return Err(crate::IronError::Other(
                    "the API key is empty, so the server would be open; refusing to start".into(),
                ));
            }
        }

        let server_config = ServerConfig {
            bind_address: config.bind_address.clone(),
            port: config.port,
            cors_enabled: false,
            cors_origins: Vec::new(),
            max_body_size: config.max_body_size,
            request_timeout_secs: config.request_timeout_secs,
            request_logging: config.request_logging,
            // Anonymous reads only when no key was configured; supplying a key means
            // the caller wants requests authenticated.
            allow_anonymous: config.api_key.is_none(),
            ..Default::default()
        };

        // Authentication for the observability routes. The gate in `route` checks the
        // key in constant time before anything reaches this layer; the key is also
        // registered here so the per-route permission checks accept it.
        let mut api_config = ApiConfig::default();
        match config.api_key {
            Some(ref key) => {
                api_config.require_auth = true;
                api_config.allow_anonymous_read = false;
                api_config
                    .keys
                    .push(ApiKey::read_only("ironmon-http-server", key.clone()));
            }
            None => {
                // No key configured. Permit unauthenticated reads, which is only
                // sound because binding defaults to loopback. What actually permits
                // the keyless read is `allow_anonymous` on the `ServerConfig` above.
                api_config.require_auth = false;
                api_config.allow_anonymous_read = true;
            }
        }

        let api = ObservabilityApi::new(api_config);
        let event_manager = crate::observability::EventManager::new(1000);
        let metric_collector = MetricCollector::new();

        let handler = Arc::new(RequestHandler::new(
            api,
            event_manager,
            metric_collector,
            server_config,
        ));

        let metric_collector = handler.metric_collector.clone();
        let loopback = is_loopback_address(&config.bind_address);

        Ok(Self {
            config,
            handler,
            metric_collector,
            #[cfg(feature = "cli")]
            sampler: sampler::Sampler::production(),
            loopback,
        })
    }

    /// Bind the listening socket. Fails on a taken or reserved port; it never falls
    /// back to another one.
    pub fn bind(&self) -> crate::Result<BoundServer> {
        let host = self
            .config
            .bind_address
            .trim_start_matches('[')
            .trim_end_matches(']');
        let port = self.config.port;
        let addrs: Vec<SocketAddr> = (host, port)
            .to_socket_addrs()
            .map_err(|e| crate::IronError::Other(format!("cannot resolve {host}:{port}: {e}")))?
            .collect();

        #[cfg(feature = "cli")]
        {
            let listener = std::net::TcpListener::bind(addrs.as_slice()).map_err(|e| {
                crate::IronError::Other(format!(
                    "cannot listen on {host}:{port}: {e}. Choose another port, or 0 for \
                     one the OS picks."
                ))
            })?;
            let local = listener
                .local_addr()
                .map_err(|e| crate::IronError::Other(format!("no local address: {e}")))?;
            listener
                .set_nonblocking(true)
                .map_err(|e| crate::IronError::Other(format!("cannot configure socket: {e}")))?;
            Ok(BoundServer { listener, local })
        }
        #[cfg(not(feature = "cli"))]
        {
            let _ = addrs;
            Err(crate::IronError::NotImplemented(
                "HTTP server requires the 'cli' feature (for tokio)".into(),
            ))
        }
    }

    /// Bind and serve (blocks until shutdown)
    #[cfg(feature = "cli")]
    pub async fn run(&self) -> crate::Result<()> {
        let bound = self.bind()?;
        self.serve(bound).await
    }

    /// Run the HTTP server (stub when cli feature is not enabled)
    #[cfg(not(feature = "cli"))]
    pub async fn run(&self) -> crate::Result<()> {
        Err(crate::IronError::NotImplemented(
            "HTTP server requires the 'cli' feature (for tokio)".into(),
        ))
    }

    /// Serve connections on a socket returned by [`HttpServer::bind`].
    #[cfg(feature = "cli")]
    pub async fn serve(&self, bound: BoundServer) -> crate::Result<()> {
        use tokio::net::TcpListener;

        let local = bound.local;
        let listener = TcpListener::from_std(bound.listener)
            .map_err(|e| crate::IronError::Other(format!("cannot register listener: {e}")))?;

        if self.config.request_logging {
            eprintln!("[iron-monitor] HTTP server listening on http://{}", local);
            eprintln!("[iron-monitor] Endpoints: /health, /api/v1/*, /api/v1/metrics/prometheus");
        }

        // The Prometheus collector runs only while the exporter is being read.
        let prometheus = PrometheusDemand::default();
        let collector = self.metric_collector.clone();
        let interval = self.config.metric_interval_secs;
        let loop_demand = prometheus.clone();
        tokio::spawn(async move {
            Self::metric_collection_loop(collector, interval, loop_demand).await;
        });

        let connection = Arc::new(Connection {
            handler: self.handler.clone(),
            sampler: self.sampler.clone(),
            prometheus,
            api_key: self.config.api_key.clone(),
            cors_origins: if self.config.cors_enabled {
                self.config.cors_origins.clone()
            } else {
                Vec::new()
            },
            loopback: self.loopback,
            request_logging: self.config.request_logging,
            deadline: std::time::Duration::from_secs(self.config.request_timeout_secs.max(1)),
        });
        let permits = Arc::new(tokio::sync::Semaphore::new(MAX_CONNECTIONS));

        loop {
            let (stream, peer_addr) = match listener.accept().await {
                Ok(conn) => conn,
                Err(e) => {
                    eprintln!("[iron-monitor] Accept error: {}", e);
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                    continue;
                }
            };
            let Ok(permit) = permits.clone().try_acquire_owned() else {
                // Over the limit: dropping the socket is the cheapest refusal.
                drop(stream);
                continue;
            };
            let connection = connection.clone();
            tokio::spawn(async move {
                let _permit = permit;
                connection.serve(stream, peer_addr).await;
            });
        }
    }

    /// Parse raw HTTP/1.1 request into ObsRequest
    #[cfg(feature = "cli")]
    #[allow(dead_code)]
    fn parse_http_request(raw: &str, client_addr: &str) -> Option<ObsRequest> {
        let mut lines = raw.lines();
        let request_line = lines.next()?;
        let parts: Vec<&str> = request_line.split_whitespace().collect();
        if parts.len() < 2 {
            return None;
        }

        let method = parts[0].to_uppercase();
        let full_path = parts[1];

        // Split path and query string
        let (path, query) = if let Some(idx) = full_path.find('?') {
            let p = &full_path[..idx];
            let q = &full_path[idx + 1..];
            let mut map = HashMap::new();
            for pair in q.split('&') {
                if let Some(eq) = pair.find('=') {
                    map.insert(
                        v1::percent_decode(&pair[..eq]).unwrap_or_else(|| pair[..eq].to_string()),
                        v1::percent_decode(&pair[eq + 1..])
                            .unwrap_or_else(|| pair[eq + 1..].to_string()),
                    );
                }
            }
            (p.to_string(), map)
        } else {
            (full_path.to_string(), HashMap::new())
        };

        // Parse headers
        let mut headers = HashMap::new();
        for line in lines {
            if line.is_empty() {
                break;
            }
            if let Some(colon) = line.find(':') {
                let key = line[..colon].trim().to_lowercase();
                let value = line[colon + 1..].trim().to_string();
                headers.insert(key, value);
            }
        }

        Some(ObsRequest {
            method,
            path,
            query,
            headers,
            body: None, // GET requests don't have bodies typically
            client_addr: client_addr.parse().ok(),
        })
    }

    #[cfg(feature = "cli")]
    /// Publish metrics under the names the bundled Grafana dashboards query.
    ///
    /// Three things were wrong here before:
    ///
    /// 1. **CPU and memory always read zero.** This called `CpuStats::new()` and
    ///    `MemoryStats::new()`, the generic constructors, rather than the platform
    ///    collectors — so on Windows the endpoint served `cpu_usage_percent 0` and
    ///    `memory_total_bytes 0` on a 93 GB machine, while GPU values were real.
    /// 2. **The names had no `ironmon_` prefix**, but every dashboard in `grafana/`
    ///    queries `ironmon_cpu_usage_percent`, `ironmon_gpu_temperature_celsius` and so
    ///    on. All three dashboards therefore rendered empty against a live server.
    /// 3. **`GpuCollection::auto_detect()` ran every interval**, re-initializing the
    ///    vendor drivers on each pass rather than once.
    ///
    /// It now reads the snapshot pipeline, which collects concurrently on its own
    /// thread and holds its handles open.
    ///
    /// The collector runs only while the Prometheus route has been read recently (see
    /// [`PROMETHEUS_IDLE`]). Its thread reads the hardware every interval whether or
    /// not anything is listening, so leaving it running would cost a server with no
    /// readers the same work as one being scraped. Dropping the collector stops and
    /// joins its thread, which happens off the async workers.
    async fn metric_collection_loop(
        collector: Arc<MetricCollector>,
        interval_secs: u64,
        demand: PrometheusDemand,
    ) {
        use crate::pipeline::{Collector, CollectorConfig};

        let interval = std::time::Duration::from_secs(interval_secs.max(1));
        let mut pipeline: Option<Collector> = None;
        loop {
            if demand.recent() {
                let running = pipeline.get_or_insert_with(|| {
                    Collector::spawn(CollectorConfig {
                        interval,
                        ..Default::default()
                    })
                });
                Self::record_snapshot(&collector, &running.handle().latest());
            } else if let Some(stopped) = pipeline.take() {
                let _ = tokio::task::spawn_blocking(move || drop(stopped)).await;
            }
            tokio::time::sleep(interval).await;
        }
    }

    /// Record one pipeline snapshot into the metric collector.
    ///
    /// Split out of `metric_collection_loop` so that it can be tested. The
    /// loop needs a live pipeline and a tokio runtime, so for as long as this
    /// logic lived inside it the only way to check a metric name or a label
    /// was to read the code -- which is how the instance ended up baked into
    /// the name (`ironmon_gpu_0_utilization_percent`) and a rate ended up under
    /// a `_total` name, both for the life of the endpoint.
    ///
    /// Takes a plain `&Snapshot`, which `Default` can construct, so a test
    /// can assert on exactly the metrics one snapshot produces.
    ///
    /// Called by the serving loop and by `tests/prometheus_exposition.rs`, both
    /// of which need features this subset does not enable.
    #[allow(dead_code)]
    pub(crate) fn record_snapshot(collector: &MetricCollector, snap: &crate::pipeline::Snapshot) {
        if let Some(ref cpu) = snap.cpu {
            collector.record("ironmon_cpu_usage_percent", (100.0 - cpu.total.idle) as f64);
            // Omit the series rather than record 0 MHz, the same way the
            // Prometheus exporter omits an absent GPU temperature.
            // This is the clock of core 0 (the first core enumerated), not of the
            // CPU. The name is kept for existing scrapers; the Prometheus exporter
            // carries every core's clock as `cpu_core_frequency_mhz`.
            if let Some(mhz) = cpu
                .cores
                .first()
                .and_then(|c| c.frequency.as_ref())
                .and_then(|f| f.current)
            {
                collector.record("ironmon_cpu_frequency_mhz", mhz as f64);
            }
        }

        if let Some(ref mem) = snap.memory {
            // Platform collectors report kilobytes; the dashboards plot bytes.
            let used = mem.ram.used as f64 * 1024.0;
            let total = mem.ram.total as f64 * 1024.0;
            collector.record("ironmon_memory_used_bytes", used);
            collector.record("ironmon_memory_total_bytes", total);
            // Recorded only when the pagefile was actually read. A swap
            // gauge of zero states that nothing is paged out, which is a
            // measurement; `used_or_zero()` manufactured it from an absence.
            if let Some(used) = mem.swap.used {
                collector.record("ironmon_swap_used_bytes", used as f64 * 1024.0);
            }
            if total > 0.0 {
                collector.record("ironmon_memory_usage_percent", (used / total) * 100.0);
            }
        }

        for (i, gpu) in snap.gpu_dynamic.iter().enumerate() {
            // A device whose query failed this tick publishes nothing rather than
            // a zero, so the dashboard shows a gap instead of a false reading.
            let Some(gpu) = gpu.as_ref() else { continue };
            // The index goes in a label, not in the metric name.
            //
            // This recorded `ironmon_gpu_0_utilization_percent`, and every
            // bundled dashboard queries
            // `ironmon_gpu_utilization_percent{gpu="0"}` -- so the panels
            // could never match, whatever the values were. Encoding an
            // instance in the name also defeats aggregation: `sum by (gpu)`
            // has nothing to group on when each card is a different metric.
            let index = i.to_string();
            let gpu_label: &[(&str, &str)] = &[("gpu", index.as_str())];
            let put = |name: &str, value: f64| {
                collector.record_with_labels(name, value, gpu_label);
            };

            if let Some(util) = gpu.utilization {
                put("ironmon_gpu_utilization_percent", util as f64);
            }
            // Recorded only where the device reported a figure, like the
            // temperature below and unlike the zeros these used to publish.
            if let Some(used) = gpu.memory.used {
                put("ironmon_gpu_memory_used_bytes", used as f64);
            }
            if let Some(total) = gpu.memory.total {
                put("ironmon_gpu_memory_total_bytes", total as f64);
            }

            if let Some(temp) = gpu.thermal.temperature {
                put("ironmon_gpu_temperature_celsius", temp as f64);
            }
            if let Some(power) = gpu.power.draw {
                put("ironmon_gpu_power_watts", power as f64 / 1000.0);
            }
            if let Some(fan) = gpu.thermal.fan_speed {
                put("ironmon_gpu_fan_speed_percent", fan as f64);
            }
            if let Some(clock) = gpu.clocks.graphics {
                put("ironmon_gpu_clock_graphics_mhz", clock as f64);
            }
            if let Some(clock) = gpu.clocks.memory {
                put("ironmon_gpu_clock_memory_mhz", clock as f64);
            }
        }

        for disk in &snap.disks {
            // The device goes in a label, for the reason above.
            //
            // And these are **rates**, which were recorded under
            // `..._bytes_total`. `DiskSnapshot` carries `read_rate` and
            // `write_rate` and no cumulative counter at all, so the name
            // promised a monotonic total and delivered bytes per second --
            // `rate()` over that in a dashboard is meaningless. The
            // cumulative figures exist, but only on the `PrometheusExporter`
            // path, which reads `io_stats` directly.
            let device: &[(&str, &str)] = &[("device", disk.name.as_str())];
            // Emitted only where a rate exists. The pipeline reports none on
            // Windows, whose disk path reads capacity and no counters.
            if let Some(rate) = disk.read_rate {
                collector.record_with_labels("ironmon_disk_read_bytes_per_sec", rate, device);
            }
            if let Some(rate) = disk.write_rate {
                collector.record_with_labels("ironmon_disk_write_bytes_per_sec", rate, device);
            }
            // Both readings, and a capacity that is genuinely zero cannot be
            // divided by. A disk whose capacity was not read emits no series
            // at all -- `0%` would scrape as a fact about a full-looking disk.
            if let (Some(used), Some(total)) = (disk.used, disk.total) {
                if total > 0 {
                    collector.record_with_labels(
                        "ironmon_disk_usage_percent",
                        (used as f64 / total as f64) * 100.0,
                        device,
                    );
                }
            }
        }

        // One series per readable sensor. A CPU that exposes none emits
        // nothing at all rather than a zero, which Prometheus already reads as
        // "not reported" -- and 0 degrees would be a specific and alarming
        // claim about a machine whose temperature nobody could measure.
        for sensor in &snap.cpu_temperatures {
            collector.record_with_labels(
                "ironmon_cpu_temperature_celsius",
                sensor.value as f64,
                &[("sensor", sensor.name.as_str())],
            );
        }

        // Cumulative per-device byte counters, which the bundled dashboards
        // query and the served endpoint could not publish until the pipeline
        // carried them. Labelled by physical device rather than by mount:
        // `DiskSnapshot` is per filesystem and these are per drive, and on
        // Windows the two do not map onto each other without a partition table
        // nobody has written. See `DiskIoSnapshot`.
        for io in &snap.disk_io {
            let device = &[("device", io.device.as_str())];
            // A counter that was not read is an absent series, not a zero one.
            // Emitting zero would make a scraper's rate() see a counter reset.
            if let Some(read) = io.read_bytes {
                collector.record_with_labels("ironmon_disk_read_bytes_total", read as f64, device);
            }
            if let Some(written) = io.write_bytes {
                collector.record_with_labels(
                    "ironmon_disk_write_bytes_total",
                    written as f64,
                    device,
                );
            }
        }

        // `_total` is a counter, and these carry the cumulative byte counts.
        //
        // They used to carry `total_rx_rate()` -- a bytes/sec figure, summed
        // over interfaces, published unlabelled under a counter's name. The
        // bundled fleet dashboard plots `rate(ironmon_network_rx_bytes_total[5m])`,
        // so it was taking the rate of change *of a rate*: near zero under
        // steady traffic, and a spike shaped like the derivative of the load
        // rather than the load. The library exporter has published the true
        // counter under this exact name all along, per interface, so the two
        // publishers disagreed about what the name meant and the reachable one
        // was wrong.
        //
        // Labelled per interface, like the library exporter and like disks here,
        // rather than summed: a sum over counters jumps backwards whenever an
        // interface disappears, and Prometheus reads that as a counter reset on
        // the whole series.
        for iface in &snap.network {
            let interface = &[("interface", iface.name.as_str())];
            collector.record_with_labels(
                "ironmon_network_rx_bytes_total",
                iface.rx_bytes as f64,
                interface,
            );
            collector.record_with_labels(
                "ironmon_network_tx_bytes_total",
                iface.tx_bytes as f64,
                interface,
            );

            // Rates go under their own names, and only once one exists: a
            // collector handed `0` cannot tell it from a quiet link.
            if let Some(rate) = iface.rx_rate {
                collector.record_with_labels("ironmon_network_rx_bytes_per_sec", rate, interface);
            }
            if let Some(rate) = iface.tx_rate {
                collector.record_with_labels("ironmon_network_tx_bytes_per_sec", rate, interface);
            }
        }
        // Only when this cycle read the table. On a cached or failed cycle the
        // table is the last good one, and its length is not a current count.
        if snap.process_sampled {
            collector.record("ironmon_process_count", snap.processes.len() as f64);
        }

        if let Some(ref stats) = snap.system_stats {
            if let Some(ref load) = stats.load_average {
                collector.record("ironmon_load_average_1m", load.one);
                collector.record("ironmon_load_average_5m", load.five);
            }
            // Queried by the bundled host dashboard, and sitting unread in the
            // same struct the load average was already taken from.
            if let Some(uptime) = stats.uptime_seconds {
                collector.record("ironmon_uptime_seconds", uptime as f64);
            }
        }
    }
}

/// How long after the last read of the Prometheus route the legacy collector keeps
/// running. Scrapes arrive every few seconds to minutes, so this is generous.
#[cfg(feature = "cli")]
const PROMETHEUS_IDLE: std::time::Duration = std::time::Duration::from_secs(300);

/// When the Prometheus route was last read. Shared between the connections, which
/// set it, and the collector loop, which reads it.
#[cfg(feature = "cli")]
#[derive(Clone, Default)]
struct PrometheusDemand(Arc<std::sync::Mutex<Option<std::time::Instant>>>);

#[cfg(feature = "cli")]
impl PrometheusDemand {
    fn touch(&self) {
        let mut last = self.0.lock().unwrap_or_else(|e| e.into_inner());
        *last = Some(std::time::Instant::now());
    }

    fn recent(&self) -> bool {
        let last = self.0.lock().unwrap_or_else(|e| e.into_inner());
        last.is_some_and(|at| at.elapsed() < PROMETHEUS_IDLE)
    }
}

/// Per-connection state, shared by every connection the server accepts.
#[cfg(feature = "cli")]
struct Connection {
    handler: Arc<RequestHandler>,
    #[cfg(feature = "cli")]
    sampler: sampler::Sampler,
    prometheus: PrometheusDemand,
    api_key: Option<String>,
    /// Empty unless CORS is enabled; then exactly the configured origins.
    cors_origins: Vec<String>,
    loopback: bool,
    request_logging: bool,
    deadline: std::time::Duration,
}

#[cfg(feature = "cli")]
enum HeadError {
    /// The head grew past `MAX_HEAD_BYTES` before its blank line.
    TooLarge,
    /// The peer went away, or failed, before a complete head arrived.
    Closed,
}

#[cfg(feature = "cli")]
impl Connection {
    async fn serve(&self, mut stream: tokio::net::TcpStream, peer: SocketAddr) {
        use tokio::io::AsyncWriteExt;

        let head = match tokio::time::timeout(self.deadline, read_head(&mut stream)).await {
            Ok(Ok(head)) => head,
            Ok(Err(HeadError::TooLarge)) => {
                let bytes = render(&Reply::error(431, "request head too large"));
                let _ = stream.write_all(&bytes).await;
                return;
            }
            Ok(Err(HeadError::Closed)) => return,
            Err(_) => {
                let bytes = render(&Reply::error(408, "request not received in time"));
                let _ = stream.write_all(&bytes).await;
                return;
            }
        };

        let reply = self.dispatch(&head, peer).await;
        let bytes = render(&reply);
        let _ = tokio::time::timeout(self.deadline, async {
            stream.write_all(&bytes).await?;
            stream.shutdown().await
        })
        .await;
    }

    async fn dispatch(&self, raw: &str, peer: SocketAddr) -> Reply {
        let Some(req) = HttpServer::parse_http_request(raw, &peer.to_string()) else {
            return Reply::error(400, "bad request");
        };
        if self.request_logging {
            // Path and query only: headers carry the bearer key and are never logged.
            eprintln!("[iron-monitor] {} {} from {}", req.method, req.path, peer);
        }
        let origin = self.allowed_origin(&req);
        let mut reply = self.route(req).await;
        if let Some(origin) = origin {
            reply
                .headers
                .push(("Access-Control-Allow-Origin".into(), origin));
            reply.headers.push(("Vary".into(), "Origin".into()));
        }
        reply
    }

    fn allowed_origin(&self, req: &ObsRequest) -> Option<String> {
        let origin = req.headers.get("origin")?;
        self.cors_origins
            .iter()
            .any(|allowed| allowed == origin)
            .then(|| origin.clone())
    }

    async fn route(&self, req: ObsRequest) -> Reply {
        // DNS rebinding: a web page can point its own name at 127.0.0.1 and read
        // responses as same-origin. CORS does not stop that, but the Host header does
        // carry the name the page used, so it is checked when bound to loopback.
        if self.loopback && !host_allowed(req.headers.get("host").map(String::as_str)) {
            return Reply::error(403, "host not allowed");
        }

        // CORS preflight answers without credentials, as browsers send it without them.
        if req.method == "OPTIONS" && self.allowed_origin(&req).is_some() {
            return Reply {
                status: 204,
                headers: vec![
                    ("Access-Control-Allow-Methods".into(), "GET".into()),
                    (
                        "Access-Control-Allow-Headers".into(),
                        "Authorization".into(),
                    ),
                    ("Access-Control-Max-Age".into(), "600".into()),
                ],
                body: String::new(),
            };
        }

        // Every route except the liveness probe needs the key, when one is set. The
        // comparison is constant time, and the failure says nothing about why.
        let authorised = match &self.api_key {
            None => true,
            Some(key) => bearer_token(&req.headers)
                .is_some_and(|presented| constant_time_eq(key.as_bytes(), presented.as_bytes())),
        };
        if !authorised {
            if req.method == "GET" && req.path == "/api/v1/health" {
                return v1::health(&self.sampler, false);
            }
            return Reply::error(401, "unauthorized").with_header("WWW-Authenticate", "Bearer");
        }

        if req.path == "/api/v1/metrics/prometheus" {
            self.prometheus.touch();
        }

        if v1::handles(&req.path) {
            let sampler = self.sampler.clone();
            let method = req.method.clone();
            let path = req.path.clone();
            let query = req.query.clone();
            return tokio::task::spawn_blocking(move || {
                v1::handle(&sampler, &method, &path, &query)
            })
            .await
            .unwrap_or_else(|_| Reply::error(500, "internal error"));
        }

        // The observability routes and the Prometheus exporter.
        let handler = self.handler.clone();
        match tokio::task::spawn_blocking(move || handler.handle(req)).await {
            Ok(response) => Reply {
                status: response.status,
                headers: response
                    .headers
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
                body: response.body,
            },
            Err(_) => Reply::error(500, "internal error"),
        }
    }
}

/// Read until the blank line that ends the request head.
#[cfg(feature = "cli")]
async fn read_head(stream: &mut tokio::net::TcpStream) -> Result<String, HeadError> {
    use tokio::io::AsyncReadExt;

    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 2048];
    loop {
        let n = stream
            .read(&mut chunk)
            .await
            .map_err(|_| HeadError::Closed)?;
        if n == 0 {
            return Err(HeadError::Closed);
        }
        buf.extend_from_slice(&chunk[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            return Ok(String::from_utf8_lossy(&buf).into_owned());
        }
        if buf.len() > MAX_HEAD_BYTES {
            return Err(HeadError::TooLarge);
        }
    }
}

/// Serialise a reply as an HTTP/1.1 response that closes the connection.
#[cfg(feature = "cli")]
fn render(reply: &Reply) -> Vec<u8> {
    let mut head = format!(
        "HTTP/1.1 {} {}\r\n",
        reply.status,
        status_text(reply.status)
    );
    let mut has_type = false;
    for (name, value) in &reply.headers {
        if name.eq_ignore_ascii_case("content-type") {
            has_type = true;
        }
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    if !has_type {
        head.push_str("Content-Type: text/plain; charset=utf-8\r\n");
    }
    head.push_str(&format!(
        "Content-Length: {}\r\nConnection: close\r\n\r\n",
        reply.body.len()
    ));
    let mut out = head.into_bytes();
    out.extend_from_slice(reply.body.as_bytes());
    out
}

/// Whether a Host header names this machine: `localhost`, `127.0.0.1` or `[::1]`,
/// with or without a port. A missing header does not match.
#[cfg(feature = "cli")]
fn host_allowed(host: Option<&str>) -> bool {
    let Some(host) = host else {
        return false;
    };
    let name = if host.starts_with('[') {
        match host.find(']') {
            Some(end) => &host[..=end],
            None => return false,
        }
    } else {
        match host.rsplit_once(':') {
            Some((name, _port)) => name,
            None => host,
        }
    };
    matches!(
        name.to_ascii_lowercase().as_str(),
        "localhost" | "127.0.0.1" | "[::1]"
    )
}

/// The token of an `Authorization: Bearer <token>` header.
#[cfg(feature = "cli")]
fn bearer_token(headers: &HashMap<String, String>) -> Option<&str> {
    let value = headers.get("authorization")?;
    let (scheme, token) = value.split_once(' ')?;
    if scheme.eq_ignore_ascii_case("bearer") {
        Some(token.trim())
    } else {
        None
    }
}

/// Equality that does not stop at the first differing byte. The length still shows,
/// which is acceptable for a key compared only against what a caller presents.
#[cfg(feature = "cli")]
fn constant_time_eq(expected: &[u8], presented: &[u8]) -> bool {
    let mut difference = (expected.len() ^ presented.len()) as u32;
    let width = expected.len().max(presented.len());
    for i in 0..width {
        let a = expected.get(i).copied().unwrap_or(0);
        let b = presented.get(i).copied().unwrap_or(0);
        difference |= (a ^ b) as u32;
    }
    difference == 0
}

#[allow(dead_code)]
fn status_text(code: u16) -> &'static str {
    match code {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        429 => "Too Many Requests",
        431 => "Request Header Fields Too Large",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "Unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "cli")]
    #[test]
    fn test_parse_http_request() {
        let raw = "GET /api/v1/health?format=json HTTP/1.1\r\n\
                   Host: localhost:8080\r\n\
                   Accept: application/json\r\n\
                   \r\n";
        let req = HttpServer::parse_http_request(raw, "127.0.0.1:12345").unwrap();
        assert_eq!(req.method, "GET");
        assert_eq!(req.path, "/api/v1/health");
        assert_eq!(req.query.get("format").map(|s| s.as_str()), Some("json"));
        assert_eq!(
            req.headers.get("host").map(|s| s.as_str()),
            Some("localhost:8080")
        );
    }

    #[cfg(feature = "cli")]
    #[test]
    fn test_parse_http_request_no_query() {
        let raw = "GET /health HTTP/1.1\r\nHost: localhost\r\n\r\n";
        let req = HttpServer::parse_http_request(raw, "127.0.0.1").unwrap();
        assert_eq!(req.path, "/health");
        assert!(req.query.is_empty());
    }

    #[test]
    fn test_config_default() {
        let config = HttpServerConfig::default();
        assert_eq!(config.port, 8080);
        assert_eq!(config.bind_address, "127.0.0.1");
        assert!(!config.cors_enabled, "CORS is off unless asked for");
        assert!(config.cors_origins.is_empty());
        assert!(config.api_key.is_none());
    }

    #[test]
    fn test_status_text() {
        assert_eq!(status_text(200), "OK");
        assert_eq!(status_text(404), "Not Found");
        assert_eq!(status_text(500), "Internal Server Error");
    }

    #[test]
    fn the_key_is_redacted_from_debug_output() {
        let config = HttpServerConfig {
            api_key: Some("sentinel-debug-key".into()),
            ..Default::default()
        };
        let shown = format!("{config:?}");
        assert!(!shown.contains("sentinel-debug-key"), "{shown}");
        assert!(shown.contains("<redacted>"));
    }

    #[test]
    fn the_key_is_not_serialised() {
        let config = HttpServerConfig {
            api_key: Some("sentinel-serde-key".into()),
            ..Default::default()
        };
        let json = serde_json::to_string(&config).unwrap();
        assert!(!json.contains("sentinel-serde-key"), "{json}");
    }

    #[test]
    fn cors_origins_are_exact_and_wildcards_refused() {
        assert!(validate_cors_origin("http://localhost:5173").is_ok());
        assert!(validate_cors_origin("https://example.com").is_ok());
        assert!(validate_cors_origin("*").unwrap_err().contains("refusing"));
        assert!(validate_cors_origin("http://*.example.com").is_err());
        assert!(
            validate_cors_origin("http://localhost/").is_err(),
            "no path"
        );
        assert!(validate_cors_origin("null").is_err());
        assert!(validate_cors_origin("ftp://host").is_err());
    }

    #[test]
    #[cfg(feature = "cli")]
    fn host_header_must_name_this_machine() {
        assert!(host_allowed(Some("localhost")));
        assert!(host_allowed(Some("localhost:9100")));
        assert!(host_allowed(Some("127.0.0.1:9100")));
        assert!(host_allowed(Some("[::1]")));
        assert!(host_allowed(Some("[::1]:9100")));
        assert!(!host_allowed(Some("rebind.example:9100")));
        assert!(!host_allowed(Some("127.0.0.1.evil.example")));
        assert!(!host_allowed(Some("[::1")), "unterminated bracket");
        assert!(!host_allowed(None), "a missing Host is not local");
    }

    /// The first scrape after idle can run before any pass has finished. That scrape
    /// must not invent a value: a snapshot with nothing in it records nothing, so the
    /// exporter has no series for it rather than a zero.
    #[test]
    fn empty_snapshot_records_no_series_rather_than_zeros() {
        let collector = MetricCollector::new();
        HttpServer::record_snapshot(&collector, &crate::pipeline::Snapshot::default());
        let text = collector.export_prometheus();
        assert!(
            !text.lines().any(|l| l.starts_with("ironmon_")),
            "an empty snapshot exported series: {text}"
        );
    }

    #[test]
    fn loopback_bind_addresses_are_recognised() {
        assert!(is_loopback_address("127.0.0.1"));
        assert!(is_loopback_address("localhost"));
        assert!(is_loopback_address("::1"));
        assert!(is_loopback_address("[::1]"));
        assert!(!is_loopback_address("0.0.0.0"));
        assert!(!is_loopback_address("192.168.1.10"));
    }

    #[test]
    #[cfg(feature = "cli")]
    fn bearer_tokens_are_parsed_and_compared_exactly() {
        let mut headers = HashMap::new();
        headers.insert("authorization".to_string(), "Bearer abc".to_string());
        assert_eq!(bearer_token(&headers), Some("abc"));
        headers.insert("authorization".to_string(), "bearer abc".to_string());
        assert_eq!(
            bearer_token(&headers),
            Some("abc"),
            "scheme is case-insensitive"
        );
        headers.insert("authorization".to_string(), "Basic abc".to_string());
        assert_eq!(bearer_token(&headers), None);

        assert!(constant_time_eq(b"key", b"key"));
        assert!(!constant_time_eq(b"key", b"kex"));
        assert!(!constant_time_eq(b"key", b"keyy"));
        assert!(!constant_time_eq(b"", b"x"));
    }
}

#[cfg(test)]
mod snapshot_recording_tests {
    use super::*;
    use crate::pipeline::{DiskSnapshot, Snapshot};

    /// Every metric a snapshot produces, with its labels, from one place.
    ///
    /// `record_snapshot` was inline in an async loop needing a live pipeline, so
    /// nothing about the names it emits could be asserted. Two defects lived
    /// there for the life of the endpoint as a result: the instance was baked
    /// into the metric name, so `ironmon_gpu_utilization_percent{gpu="0"}` — what
    /// every bundled dashboard queries — never matched anything, and a disk
    /// *rate* was recorded under a `_total` name.
    fn rendered(snap: &Snapshot) -> String {
        let collector = MetricCollector::new();
        HttpServer::record_snapshot(&collector, snap);
        collector.export_prometheus()
    }

    #[test]
    fn disk_metrics_carry_a_device_label_and_a_rate_is_named_as_one() {
        let snap = Snapshot {
            disks: vec![DiskSnapshot {
                name: "PhysicalDrive0".into(),
                total: Some(1_000),
                used: Some(250),
                read_rate: Some(4096.0),
                write_rate: Some(512.0),
                ..Default::default()
            }],
            ..Default::default()
        };

        let text = rendered(&snap);

        assert!(
            text.contains("ironmon_disk_read_bytes_per_sec{device=\"PhysicalDrive0\"} 4096"),
            "a per-device rate needs a device label and a rate's name: {text}"
        );
        assert!(
            text.contains("ironmon_disk_usage_percent{device=\"PhysicalDrive0\"} 25"),
            "usage should be labelled and computed from used/total: {text}"
        );
        assert!(
            !text.contains("ironmon_disk_0_"),
            "an index belongs in a label, not in the metric name: {text}"
        );
        // A `_total` may be published only from a genuine cumulative counter,
        // and now there is one: `Snapshot::disk_io`. This snapshot's `disks`
        // carry rates and no counters, so nothing may claim a total from them.
        assert!(
            !text.contains("ironmon_disk_read_bytes_total"),
            "the counters come from `disk_io`, which this snapshot leaves \
             empty, so no total may appear: {text}"
        );
    }

    /// A readable CPU sensor is published; an absent one emits nothing.
    ///
    /// The second half is the point. This crate's development machine reads
    /// zero sensors through all four Windows paths, and a `0` there would be a
    /// specific and alarming claim about a temperature nobody could measure.
    #[test]
    fn cpu_temperature_is_published_per_sensor_or_not_at_all() {
        use crate::hwmon::{HwSensor, HwSensorType, HwType};

        assert!(
            !rendered(&Snapshot::default()).contains("ironmon_cpu_temperature_celsius"),
            "a snapshot with no sensors must publish no temperature"
        );

        let snap = Snapshot {
            cpu_temperatures: vec![HwSensor {
                name: "Core 0".into(),
                value: 47.5,
                min: None,
                max: None,
                sensor_type: HwSensorType::Temperature,
                hardware_type: HwType::Cpu,
            }],
            ..Default::default()
        };
        let text = rendered(&snap);
        assert!(
            text.contains("ironmon_cpu_temperature_celsius{sensor=\"Core 0\"} 47.5"),
            "a sensor needs its own label and its reading: {text}"
        );
    }

    /// The cumulative counters are published from `disk_io`, per device.
    ///
    /// They are keyed by physical drive while `disks` is keyed by filesystem,
    /// which is why they travel in a separate list -- see `DiskIoSnapshot`.
    #[test]
    fn cumulative_disk_counters_are_published_per_device() {
        let snap = Snapshot {
            disk_io: vec![crate::pipeline::DiskIoSnapshot {
                device: "PhysicalDrive0".into(),
                read_bytes: Some(8_111_574_792_192),
                write_bytes: Some(5_613_750_109_184),
            }],
            ..Default::default()
        };

        let text = rendered(&snap);
        assert!(
            text.contains("ironmon_disk_read_bytes_total{device=\"PhysicalDrive0\"} 8111574792192"),
            "a cumulative counter needs its device label and its full value: {text}"
        );
        assert!(
            text.contains(
                "ironmon_disk_write_bytes_total{device=\"PhysicalDrive0\"} 5613750109184"
            ),
            "the write counter had no counterpart: {text}"
        );
    }

    /// A snapshot with nothing in it records nothing, rather than zeros.
    #[test]
    fn an_empty_snapshot_records_only_what_it_knows() {
        let text = rendered(&Snapshot::default());

        for absent in [
            "ironmon_cpu_usage_percent",
            "ironmon_memory_used_bytes",
            "ironmon_gpu_utilization_percent",
            "ironmon_network_rx_bytes_total",
        ] {
            assert!(
                !text.contains(absent),
                "{absent} was recorded from a snapshot that carries no such \
                 reading: {text}"
            );
        }
    }
}

#[cfg(test)]
mod dashboard_coverage_tests {
    use super::*;
    use crate::pipeline::{DiskSnapshot, NetSnapshot, Snapshot};

    /// What the served endpoint offers against what the dashboards ask for.
    ///
    /// The Prometheus endpoint renders `MetricCollector`, populated by
    /// `record_snapshot` — not `PrometheusExporter`. So the names this function
    /// emits are the names a scrape of a running server actually returns, and
    /// the gap against `grafana/` is the number of panels that draw nothing.
    ///
    /// Reported rather than asserted at a fixed number: a snapshot assembled
    /// here cannot carry every reading a real machine has, so this documents
    /// coverage and fails only on a name the recorder never emits at all.
    #[test]
    fn the_served_endpoint_covers_the_dashboard_metrics_it_can() {
        use std::collections::BTreeSet;

        let snap = Snapshot {
            process_sampled: true,
            disks: vec![DiskSnapshot {
                name: "PhysicalDrive0".into(),
                total: Some(1_000),
                used: Some(250),
                read_rate: Some(1.0),
                write_rate: Some(1.0),
                ..Default::default()
            }],
            network: vec![NetSnapshot {
                name: "Ethernet".into(),
                rx_rate: Some(1.0),
                tx_rate: Some(1.0),
                ..Default::default()
            }],
            ..Default::default()
        };

        let collector = MetricCollector::new();
        HttpServer::record_snapshot(&collector, &snap);
        let text = collector.export_prometheus();
        let emitted: BTreeSet<&str> = text
            .lines()
            .filter_map(|l| l.split(&['{', ' '][..]).next())
            .filter(|s| !s.is_empty())
            .collect();

        // Names this snapshot carries the data for. CPU, memory, GPU and
        // load average need readings a synthetic snapshot has no source for.
        for required in [
            "ironmon_disk_usage_percent",
            "ironmon_disk_read_bytes_per_sec",
            "ironmon_network_rx_bytes_total",
            "ironmon_process_count",
        ] {
            assert!(
                emitted.contains(required),
                "record_snapshot did not emit {required}, which the bundled \
                 dashboards query and this snapshot carries: {emitted:#?}"
            );
        }
    }
}
