//! HTTP Server for IronMonitor REST API
//!
//! Provides a lightweight HTTP/1.1 server built on tokio that exposes all
//! monitoring data via the Observability API. Supports JSON endpoints,
//! Prometheus metrics, health checks, and OpenAPI spec.
//!
//! # Examples
//!
//! ```no_run
//! use ironmonlib::http_server::{HttpServer, HttpServerConfig};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = HttpServerConfig {
//!         bind_address: "0.0.0.0".into(),
//!         port: 8080,
//!         ..Default::default()
//!     };
//!     let server = HttpServer::new(config)?;
//!     println!("Listening on http://0.0.0.0:8080");
//!     server.run().await?;
//!     Ok(())
//! }
//! ```

use crate::observability::permissions::ApiKey;
use crate::observability::{
    ApiConfig, HttpRequest as ObsRequest, MetricCollector, ObservabilityApi, RequestHandler,
    ServerConfig,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// HTTP server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpServerConfig {
    /// Bind address (default: "127.0.0.1")
    pub bind_address: String,
    /// Port (default: 8080)
    pub port: u16,
    /// CORS enabled (default: true)
    pub cors_enabled: bool,
    /// Allowed CORS origins (default: ["*"])
    pub cors_origins: Vec<String>,
    /// Maximum request body size in bytes (default: 1MB)
    pub max_body_size: usize,
    /// Request timeout in seconds (default: 30)
    pub request_timeout_secs: u64,
    /// Whether to log requests to stderr (default: true)
    pub request_logging: bool,
    /// API key requirement (None = no auth)
    pub api_key: Option<String>,
    /// Metric collection interval in seconds (default: 5)
    pub metric_interval_secs: u64,
}

impl Default for HttpServerConfig {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1".into(),
            port: 8080,
            cors_enabled: true,
            cors_origins: vec!["*".into()],
            max_body_size: 1_048_576,
            request_timeout_secs: 30,
            request_logging: true,
            api_key: None,
            metric_interval_secs: 5,
        }
    }
}

/// HTTP server that exposes IronMonitor data
#[allow(dead_code)]
pub struct HttpServer {
    config: HttpServerConfig,
    handler: Arc<RequestHandler>,
    metric_collector: Arc<MetricCollector>,
}

impl HttpServer {
    /// Create a new HTTP server with the given configuration
    pub fn new(config: HttpServerConfig) -> crate::Result<Self> {
        let server_config = ServerConfig {
            bind_address: config.bind_address.clone(),
            port: config.port,
            cors_enabled: config.cors_enabled,
            cors_origins: config.cors_origins.clone(),
            max_body_size: config.max_body_size,
            request_timeout_secs: config.request_timeout_secs,
            request_logging: config.request_logging,
            // Anonymous reads only when no key was configured; supplying a key means
            // the caller wants requests authenticated.
            allow_anonymous: config.api_key.is_none(),
            ..Default::default()
        };

        // Authentication.
        //
        // This previously accepted `config.api_key` and threw it away (`let _ = key`)
        // while still setting `require_auth`, and the no-key branch used
        // `ApiConfig::default()`, which also requires auth. The result was a server
        // that answered every request with 401 in every configuration — including the
        // Prometheus scrape the bundled Grafana dashboards depend on.
        let mut api_config = ApiConfig::default();
        match config.api_key {
            Some(ref key) => {
                // Neither field is consulted -- see their docs. The gate
                // above decides anonymous access via `allow_anonymous`, which
                // is already `false` when a key is configured. These record
                // intent and nothing else.
                api_config.require_auth = true;
                api_config.allow_anonymous_read = false;
                api_config
                    .keys
                    .push(ApiKey::read_only("ironmon-http-server", key.clone()));
            }
            None => {
                // No key configured. Permit unauthenticated reads, which is only
                // sound because binding defaults to loopback — a local user can read
                // this telemetry by running ironmon directly anyway. Callers exposing
                // the server on a routable address are expected to supply a key.
                // As above: inert. What actually permits the keyless read
                // is `allow_anonymous: config.api_key.is_none()` on the
                // `ServerConfig` built above.
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

        Ok(Self {
            config,
            handler,
            metric_collector,
        })
    }

    /// Run the HTTP server (blocks until shutdown)
    #[cfg(feature = "cli")]
    pub async fn run(&self) -> crate::Result<()> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let addr = format!("{}:{}", self.config.bind_address, self.config.port);
        let listener = TcpListener::bind(&addr)
            .await
            .map_err(|e| crate::IronError::Other(format!("Failed to bind to {}: {}", addr, e)))?;

        if self.config.request_logging {
            eprintln!("[iron-monitor] HTTP server listening on http://{}", addr);
            eprintln!("[iron-monitor] Endpoints: /health, /api/v1/*, /api/v1/metrics/prometheus");
        }

        // Start metric collection background task
        let collector = self.metric_collector.clone();
        let interval = self.config.metric_interval_secs;
        tokio::spawn(async move {
            Self::metric_collection_loop(collector, interval).await;
        });

        loop {
            let (mut stream, peer_addr) = match listener.accept().await {
                Ok(conn) => conn,
                Err(e) => {
                    eprintln!("[iron-monitor] Accept error: {}", e);
                    continue;
                }
            };

            let handler = self.handler.clone();
            let log = self.config.request_logging;
            let max_body = self.config.max_body_size;
            let cors = self.config.cors_enabled;
            let cors_origins = self.config.cors_origins.clone();

            tokio::spawn(async move {
                let mut buf = vec![0u8; max_body.min(65536)];
                let n = match stream.read(&mut buf).await {
                    Ok(n) if n > 0 => n,
                    _ => return,
                };

                let raw = String::from_utf8_lossy(&buf[..n]);

                // Parse HTTP request
                let obs_request = match Self::parse_http_request(&raw, &peer_addr.to_string()) {
                    Some(req) => req,
                    None => {
                        let resp =
                            b"HTTP/1.1 400 Bad Request\r\nContent-Length: 11\r\n\r\nBad Request";
                        let _ = stream.write_all(resp).await;
                        return;
                    }
                };

                if log {
                    eprintln!(
                        "[iron-monitor] {} {} {} from {}",
                        obs_request.method,
                        obs_request.path,
                        obs_request
                            .query
                            .iter()
                            .map(|(k, v)| format!("{}={}", k, v))
                            .collect::<Vec<_>>()
                            .join("&"),
                        peer_addr,
                    );
                }

                // Handle CORS preflight
                if obs_request.method == "OPTIONS" && cors {
                    let origin = cors_origins.first().cloned().unwrap_or_else(|| "*".into());
                    let resp = format!(
                        "HTTP/1.1 204 No Content\r\n\
                         Access-Control-Allow-Origin: {}\r\n\
                         Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
                         Access-Control-Allow-Headers: Content-Type, Authorization\r\n\
                         Access-Control-Max-Age: 86400\r\n\
                         Content-Length: 0\r\n\r\n",
                        origin
                    );
                    let _ = stream.write_all(resp.as_bytes()).await;
                    return;
                }

                // Dispatch through RequestHandler
                let obs_response = handler.handle(obs_request);

                // Build HTTP response
                let mut resp = format!(
                    "HTTP/1.1 {} {}\r\n",
                    obs_response.status,
                    status_text(obs_response.status)
                );

                for (key, value) in &obs_response.headers {
                    resp.push_str(&format!("{}: {}\r\n", key, value));
                }

                if cors {
                    let origin = cors_origins.first().cloned().unwrap_or_else(|| "*".into());
                    resp.push_str(&format!("Access-Control-Allow-Origin: {}\r\n", origin));
                }

                resp.push_str(&format!("Content-Length: {}\r\n", obs_response.body.len()));
                resp.push_str("Connection: close\r\n");
                resp.push_str("\r\n");
                resp.push_str(&obs_response.body);

                let _ = stream.write_all(resp.as_bytes()).await;
            });
        }
    }

    /// Run the HTTP server (stub when cli feature is not enabled)
    #[cfg(not(feature = "cli"))]
    pub async fn run(&self) -> crate::Result<()> {
        Err(crate::IronError::NotImplemented(
            "HTTP server requires the 'cli' feature (for tokio)".into(),
        ))
    }

    /// Parse raw HTTP/1.1 request into ObsRequest
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
                    map.insert(pair[..eq].to_string(), pair[eq + 1..].to_string());
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
    async fn metric_collection_loop(collector: Arc<MetricCollector>, interval_secs: u64) {
        use crate::pipeline::{Collector, CollectorConfig};

        let pipeline = Collector::spawn(CollectorConfig {
            interval: std::time::Duration::from_secs(interval_secs.max(1)),
            ..Default::default()
        });
        let snapshots = pipeline.handle();

        loop {
            Self::record_snapshot(&collector, &snapshots.latest());
            tokio::time::sleep(tokio::time::Duration::from_secs(interval_secs)).await;
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
    pub(crate) fn record_snapshot(collector: &MetricCollector, snap: &crate::pipeline::Snapshot) {
        if let Some(ref cpu) = snap.cpu {
            collector.record("ironmon_cpu_usage_percent", (100.0 - cpu.total.idle) as f64);
            // Omit the series rather than record 0 MHz, the same way the
            // Prometheus exporter omits an absent GPU temperature.
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
            if disk.total > 0 {
                collector.record_with_labels(
                    "ironmon_disk_usage_percent",
                    (disk.used as f64 / disk.total as f64) * 100.0,
                    device,
                );
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
            collector.record_with_labels(
                "ironmon_disk_read_bytes_total",
                io.read_bytes as f64,
                device,
            );
            collector.record_with_labels(
                "ironmon_disk_write_bytes_total",
                io.write_bytes as f64,
                device,
            );
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
        collector.record("ironmon_process_count", snap.processes.len() as f64);

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
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        _ => "Unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(config.cors_enabled);
    }

    #[test]
    fn test_status_text() {
        assert_eq!(status_text(200), "OK");
        assert_eq!(status_text(404), "Not Found");
        assert_eq!(status_text(500), "Internal Server Error");
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
                total: 1_000,
                used: 250,
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
                read_bytes: 8_111_574_792_192,
                write_bytes: 5_613_750_109_184,
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
            disks: vec![DiskSnapshot {
                name: "PhysicalDrive0".into(),
                total: 1_000,
                used: 250,
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
