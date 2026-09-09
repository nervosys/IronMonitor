//! Prometheus Metrics Exporter
//!
//! Exports IronMonitor metrics in Prometheus exposition format with proper
//! `# HELP`, `# TYPE` annotations and label support. Compatible with
//! Prometheus, Grafana, and other metric collection systems.
//!
//! # Examples
//!
//! ```no_run
//! use ironmonlib::prometheus::{PrometheusExporter, MetricFamily};
//!
//! let mut exporter = PrometheusExporter::new("ironmon");
//!
//! // Collect system metrics
//! exporter.collect_system_metrics();
//!
//! // Export in Prometheus text format
//! let output = exporter.export();
//! println!("{}", output);
//! // Output:
//! // # HELP ironmon_cpu_usage_percent CPU utilization percentage
//! // # TYPE ironmon_cpu_usage_percent gauge
//! // ironmon_cpu_usage_percent 42.5
//! // ...
//! ```

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Prometheus metric type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MetricType {
    /// A gauge is a metric that represents a single numerical value that can go up and down
    Gauge,
    /// A counter is a metric that represents a single monotonically increasing counter
    Counter,
    /// A histogram samples observations and counts them in configurable buckets
    Histogram,
    /// A summary is similar to a histogram but calculates configurable quantiles
    Summary,
    /// Untyped metric
    Untyped,
}

impl std::fmt::Display for MetricType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Gauge => write!(f, "gauge"),
            Self::Counter => write!(f, "counter"),
            Self::Histogram => write!(f, "histogram"),
            Self::Summary => write!(f, "summary"),
            Self::Untyped => write!(f, "untyped"),
        }
    }
}

/// A single metric sample with optional labels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricSample {
    /// Metric name suffix (empty for simple metrics)
    pub suffix: String,
    /// Label key-value pairs
    pub labels: BTreeMap<String, String>,
    /// Metric value
    pub value: f64,
}

/// A complete metric family with metadata and samples
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricFamily {
    /// Metric name
    pub name: String,
    /// Help text
    pub help: String,
    /// Metric type
    pub metric_type: MetricType,
    /// Samples
    pub samples: Vec<MetricSample>,
}

impl MetricFamily {
    /// Create a new gauge metric
    pub fn gauge(name: &str, help: &str, value: f64) -> Self {
        Self {
            name: name.to_string(),
            help: help.to_string(),
            metric_type: MetricType::Gauge,
            samples: vec![MetricSample {
                suffix: String::new(),
                labels: BTreeMap::new(),
                value,
            }],
        }
    }

    /// Create a gauge with labels
    pub fn gauge_with_labels(
        name: &str,
        help: &str,
        value: f64,
        labels: BTreeMap<String, String>,
    ) -> Self {
        Self {
            name: name.to_string(),
            help: help.to_string(),
            metric_type: MetricType::Gauge,
            samples: vec![MetricSample {
                suffix: String::new(),
                labels,
                value,
            }],
        }
    }

    /// Create a counter metric
    /// A counter carrying labels.
    ///
    /// Without this, `collect_network_metrics` built an `interface` label and
    /// then called [`Self::counter`], which drops it -- so every interface
    /// emitted `ironmon_network_rx_bytes_total` with **no labels and a different
    /// value**. Twenty identical series in one scrape is not a wrong number, it
    /// is a malformed exposition: Prometheus rejects a scrape containing
    /// duplicate samples, so the network section could take the whole endpoint
    /// down with it.
    pub fn counter_with_labels(
        name: &str,
        help: &str,
        value: f64,
        labels: BTreeMap<String, String>,
    ) -> Self {
        let mut family = Self::counter(name, help, value);
        if let Some(sample) = family.samples.first_mut() {
            sample.labels = labels;
        }
        family
    }

    pub fn counter(name: &str, help: &str, value: f64) -> Self {
        Self {
            name: name.to_string(),
            help: help.to_string(),
            metric_type: MetricType::Counter,
            samples: vec![MetricSample {
                suffix: String::new(),
                labels: BTreeMap::new(),
                value,
            }],
        }
    }

    /// Add a labeled sample to this family
    pub fn add_sample(&mut self, value: f64, labels: BTreeMap<String, String>) {
        self.samples.push(MetricSample {
            suffix: String::new(),
            labels,
            value,
        });
    }

    /// Format this metric family in Prometheus exposition format
    pub fn format(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("# HELP {} {}\n", self.name, self.help));
        out.push_str(&format!("# TYPE {} {}\n", self.name, self.metric_type));

        for sample in &self.samples {
            let full_name = if sample.suffix.is_empty() {
                self.name.clone()
            } else {
                format!("{}_{}", self.name, sample.suffix)
            };

            if sample.labels.is_empty() {
                out.push_str(&format!("{} {}\n", full_name, format_value(sample.value)));
            } else {
                let label_str: Vec<String> = sample
                    .labels
                    .iter()
                    .map(|(k, v)| format!("{}=\"{}\"", k, escape_label_value(v)))
                    .collect();
                out.push_str(&format!(
                    "{}{{{}}} {}\n",
                    full_name,
                    label_str.join(","),
                    format_value(sample.value)
                ));
            }
        }

        out
    }
}

/// Prometheus metrics exporter for IronMonitor
pub struct PrometheusExporter {
    /// Namespace prefix for all metrics
    prefix: String,
    /// Collected metric families
    families: Vec<MetricFamily>,
}

impl PrometheusExporter {
    /// Create a new exporter with the given namespace prefix
    pub fn new(prefix: &str) -> Self {
        Self {
            prefix: sanitize_metric_name(prefix),
            families: Vec::new(),
        }
    }

    /// Add a metric family
    /// Add a family, merging into one that already carries this name.
    ///
    /// The Prometheus text format allows **one** `# HELP` and one `# TYPE` per
    /// metric name, and this pushed a whole new family per sample -- so an
    /// export repeated the header for every disk, every GPU and every network
    /// interface. On the machine this was written on:
    ///
    /// ```text
    ///  6x  # HELP ironmon_disk_used_bytes
    ///  3x  # HELP ironmon_gpu_utilization_percent
    /// 20x  # HELP ironmon_network_rx_bytes_total
    /// ```
    ///
    /// `MetricFamily` already held a `Vec<MetricSample>`, so the structure was
    /// right and only the insertion was wrong.
    pub fn add(&mut self, family: MetricFamily) {
        if let Some(existing) = self.families.iter_mut().find(|f| f.name == family.name) {
            existing.samples.extend(family.samples);
            return;
        }
        self.families.push(family);
    }

    /// Collect all available system metrics
    pub fn collect_system_metrics(&mut self) {
        self.collect_cpu_metrics();
        self.collect_memory_metrics();
        self.collect_gpu_metrics();
        self.collect_disk_metrics();
        self.collect_network_metrics();
        self.collect_cpu_temperature_metrics();
        self.collect_uptime_metrics();
        self.collect_system_load_metrics();
        self.collect_profile_metrics();
    }

    /// Collect profile-inspector metrics — total/writable settings, deviations
    /// by risk, cache effectiveness. Useful for fleet drift tracking.
    pub fn collect_profile_metrics(&mut self) {
        let mut inspector = crate::profile::cache::CachedProfileInspector::new();
        let snapshot = inspector.snapshot_all();

        // Per-subsystem group + setting counts.
        let mut group_family = MetricFamily {
            name: self.prefixed("profile_groups_total"),
            help: "Number of profile groups per hardware subsystem".into(),
            metric_type: MetricType::Gauge,
            samples: Vec::new(),
        };
        let mut setting_family = MetricFamily {
            name: self.prefixed("profile_settings_total"),
            help: "Number of profile settings per hardware subsystem".into(),
            metric_type: MetricType::Gauge,
            samples: Vec::new(),
        };
        for (sub, groups) in &snapshot.providers {
            let mut labels = BTreeMap::new();
            labels.insert("subsystem".into(), sub.as_str().to_string());
            group_family.samples.push(MetricSample {
                suffix: String::new(),
                labels: labels.clone(),
                value: groups.len() as f64,
            });
            let settings: usize = groups.iter().map(|g| g.settings.len()).sum();
            setting_family.samples.push(MetricSample {
                suffix: String::new(),
                labels,
                value: settings as f64,
            });
        }
        if !group_family.samples.is_empty() {
            self.add(group_family);
        }
        if !setting_family.samples.is_empty() {
            self.add(setting_family);
        }

        // Deviations by risk.
        let deviations = crate::profile::deviation::deviations_from_default(&snapshot);
        let mut by_risk: BTreeMap<&'static str, u64> = BTreeMap::new();
        by_risk.insert("dangerous", 0);
        by_risk.insert("moderate", 0);
        by_risk.insert("safe", 0);
        by_risk.insert("informational", 0);
        for d in &deviations {
            let key = match d.risk {
                crate::profile::SettingRisk::Dangerous => "dangerous",
                crate::profile::SettingRisk::Moderate => "moderate",
                crate::profile::SettingRisk::Safe => "safe",
                crate::profile::SettingRisk::Informational => "informational",
            };
            *by_risk.entry(key).or_insert(0) += 1;
        }
        let mut dev_family = MetricFamily {
            name: self.prefixed("profile_deviations_count"),
            help: "Settings whose current value differs from declared default, by risk band".into(),
            metric_type: MetricType::Gauge,
            samples: Vec::new(),
        };
        for (risk, count) in by_risk {
            let mut labels = BTreeMap::new();
            labels.insert("risk".into(), risk.into());
            dev_family.samples.push(MetricSample {
                suffix: String::new(),
                labels,
                value: count as f64,
            });
        }
        self.add(dev_family);

        // Writable settings count.
        self.add(MetricFamily::gauge(
            &self.prefixed("profile_writable_handlers_total"),
            "Number of registered apply handlers (writable setting ids) on this build",
            crate::profile::apply::writable_setting_ids().len() as f64,
        ));

        // Cache effectiveness.
        self.add(MetricFamily::counter(
            &self.prefixed("profile_cache_hits_total"),
            "ProfileInspector cache hits (process-global)",
            crate::profile::cache::CACHE_STATS.hits() as f64,
        ));
        self.add(MetricFamily::counter(
            &self.prefixed("profile_cache_misses_total"),
            "ProfileInspector cache misses (process-global)",
            crate::profile::cache::CACHE_STATS.misses() as f64,
        ));
    }

    fn prefixed(&self, name: &str) -> String {
        format!("{}_{}", self.prefix, name)
    }

    fn collect_cpu_metrics(&mut self) {
        // Reads the platform. This exported `100 - idle` from a zero-constructor,
        // so the CPU gauge was always 0% on every scrape.
        if let Ok(cpu) = crate::stats::platform_cpu_stats() {
            self.add(MetricFamily::gauge(
                &self.prefixed("cpu_usage_percent"),
                "Total CPU utilization percentage",
                100.0_f64 - cpu.total.idle as f64,
            ));

            // Per-core utilization
            let mut per_core = MetricFamily {
                name: self.prefixed("cpu_core_usage_percent"),
                help: "Per-core CPU utilization percentage".into(),
                metric_type: MetricType::Gauge,
                samples: Vec::new(),
            };
            // One series per core that reported its idle time.
            //
            // `core.idle.unwrap_or(100.0)` published `100 - 100 = 0%` for a
            // core whose idle time was not read -- a measured, idle core, on a
            // gauge a dashboard graphs. The same reasoning as the temperature
            // block below: a core that reports nothing emits nothing, which
            // Prometheus already reads as "not reported", and the
            // `samples.is_empty()` guard below drops the family entirely when
            // no core answered.
            //
            // The tsdb writer and `ironmon serve`'s snapshot path both had this
            // and both were fixed; this is the third of eight copies.
            for core in &cpu.cores {
                let Some(idle) = core.idle else {
                    continue;
                };
                let usage = 100.0 - idle as f64;
                let mut labels = BTreeMap::new();
                labels.insert("core".into(), core.id.to_string());
                per_core.samples.push(MetricSample {
                    suffix: String::new(),
                    labels,
                    value: usage,
                });
            }
            if !per_core.samples.is_empty() {
                self.add(per_core);
            }

            self.add(MetricFamily::gauge(
                &self.prefixed("cpu_cores_total"),
                "Total number of CPU cores",
                cpu.cores.len() as f64,
            ));

            // Queried by the bundled dashboards, and available from the reading
            // already in hand. Emitted only when a clock was actually read --
            // `CpuFrequency::current` is `Option` for the reason set out on it,
            // and a gauge of 0 MHz is a claim that the cores have stopped.
            if let Some(mhz) = cpu
                .cores
                .first()
                .and_then(|c| c.frequency.as_ref())
                .and_then(|f| f.current)
            {
                self.add(MetricFamily::gauge(
                    &self.prefixed("cpu_frequency_mhz"),
                    "Current CPU clock frequency in MHz",
                    mhz as f64,
                ));
            }
        }
    }

    /// Load average and process count.
    ///
    /// Four metrics the bundled dashboards query that this exporter simply had
    /// never been taught. Unlike the served endpoint's three gaps, none of these
    /// were blocked on anything: the exporter collects from the system directly
    /// and could always have read them. They went unnoticed because the coverage
    /// test accepted a name published by *either* renderer, and `http_server.rs`
    /// publishes all four -- see `tests/prometheus_exposition.rs`.
    ///
    /// Each is emitted only where the platform reports it. Load average is a
    /// Unix quantity and Windows has none, so those two series are simply absent
    /// there; Prometheus already reads an absent series as "not reported", which
    /// is what is true.
    fn collect_system_load_metrics(&mut self) {
        let Ok(stats) = crate::system_stats::SystemStats::new() else {
            return;
        };

        if let Some(ref load) = stats.load_average {
            self.add(MetricFamily::gauge(
                &self.prefixed("load_average_1m"),
                "System load average over 1 minute",
                load.one,
            ));
            self.add(MetricFamily::gauge(
                &self.prefixed("load_average_5m"),
                "System load average over 5 minutes",
                load.five,
            ));
        }

        if let Some(total) = stats.total_processes {
            self.add(MetricFamily::gauge(
                &self.prefixed("process_count"),
                "Total number of processes",
                total as f64,
            ));
        }
    }

    fn collect_memory_metrics(&mut self) {
        // Reads the platform, for the reason above.
        if let Ok(mem) = crate::stats::platform_memory_stats() {
            // `RamInfo` is in KB and every one of these names ends in
            // `_bytes`, so each figure is scaled -- which the swap block below
            // has always done, with a comment saying why, three lines further
            // down. The RAM figures above it did not, so this exporter reported
            // a 93.6 GiB machine as having 98 MB of memory: the right number
            // under the wrong unit, off by exactly 1024, which is small enough
            // to look like a plausible reading of something.
            //
            // Caught by `tests/prometheus_agreement.rs`, which compares this
            // exporter's stable quantities against the ontology's. The served
            // endpoint in `http_server.rs` reads a different type and was
            // right; the two renderers had disagreed by a factor of 1024 for
            // as long as both have existed.
            const KB: u64 = 1024;
            self.add(MetricFamily::gauge(
                &self.prefixed("memory_total_bytes"),
                "Total physical memory in bytes",
                (mem.ram.total * KB) as f64,
            ));
            self.add(MetricFamily::gauge(
                &self.prefixed("memory_used_bytes"),
                "Used physical memory in bytes",
                (mem.ram.used * KB) as f64,
            ));
            self.add(MetricFamily::gauge(
                &self.prefixed("memory_free_bytes"),
                "Free physical memory in bytes",
                (mem.ram.free * KB) as f64,
            ));
            if mem.ram.total > 0 {
                self.add(MetricFamily::gauge(
                    &self.prefixed("memory_usage_percent"),
                    "Memory utilization percentage",
                    (mem.ram.used as f64 / mem.ram.total as f64) * 100.0,
                ));
            }
            // `swap_used_bytes` is queried by the bundled host dashboard and was
            // never published. It is `Option` since 15a60ab -- emitted only when
            // the pagefile was actually read, because a swap gauge of zero is a
            // claim that nothing is paged out.
            if let Some(used) = mem.swap.used {
                self.add(MetricFamily::gauge(
                    &self.prefixed("swap_used_bytes"),
                    "Swap or pagefile bytes in use",
                    // `SwapInfo` is in KB; the metric name says bytes. Same
                    // scaling as the RAM figures above.
                    (used * KB) as f64,
                ));
            }
        }
    }

    /// CPU temperature, which the bundled dashboards query and nothing
    /// published.
    ///
    /// `hwmon::read_cpu_temperatures` returns one reading per sensor the
    /// platform exposes, and each carries its own label so several packages or
    /// cores do not collide into one series. Nothing is emitted where no sensor
    /// is readable -- on Windows that is the ordinary case without a signed
    /// kernel driver, and `read_cpu_temperatures` returns an empty list there
    /// rather than a zero, so the loop simply does not run.
    fn collect_cpu_temperature_metrics(&mut self) {
        for sensor in crate::hwmon::read_cpu_temperatures() {
            let celsius = sensor.value;
            let mut labels = BTreeMap::new();
            labels.insert("sensor".into(), sensor.name.clone());
            self.add(MetricFamily::gauge_with_labels(
                &self.prefixed("cpu_temperature_celsius"),
                "CPU temperature in degrees Celsius",
                celsius as f64,
                labels,
            ));
        }
    }

    /// Uptime, which the bundled dashboards query and nothing published.
    ///
    /// `stats::uptime` has had a platform implementation on all three targets
    /// the whole time; no exporter had called it.
    fn collect_uptime_metrics(&mut self) {
        if let Ok(uptime) = crate::stats::uptime() {
            self.add(MetricFamily::gauge(
                &self.prefixed("uptime_seconds"),
                "Seconds since boot",
                uptime.as_secs() as f64,
            ));
        }
    }

    fn collect_gpu_metrics(&mut self) {
        if let Ok(gpus) = crate::GpuCollection::auto_detect() {
            if let Ok(snapshots) = gpus.snapshot_all() {
                // GPU count
                self.add(MetricFamily::gauge(
                    &self.prefixed("gpu_count"),
                    "Number of detected GPUs",
                    snapshots.len() as f64,
                ));

                for (i, info) in snapshots.iter().enumerate() {
                    let mut base_labels = BTreeMap::new();
                    base_labels.insert("gpu".into(), i.to_string());
                    base_labels.insert("name".into(), info.static_info.name.clone());
                    base_labels.insert("vendor".into(), info.static_info.vendor.to_string());

                    // Utilization, where the device reports one.
                    if let Some(util) = info.dynamic_info.utilization {
                        self.add(MetricFamily::gauge_with_labels(
                            &self.prefixed("gpu_utilization_percent"),
                            "GPU compute utilization percentage",
                            util as f64,
                            base_labels.clone(),
                        ));
                    }

                    // Temperature
                    if let Some(temp) = info.dynamic_info.thermal.temperature {
                        self.add(MetricFamily::gauge_with_labels(
                            &self.prefixed("gpu_temperature_celsius"),
                            "GPU temperature in degrees Celsius",
                            temp as f64,
                            base_labels.clone(),
                        ));
                    }

                    // Power
                    if let Some(power) = info.dynamic_info.power.draw {
                        self.add(MetricFamily::gauge_with_labels(
                            &self.prefixed("gpu_power_watts"),
                            "GPU power draw in watts",
                            power as f64 / 1000.0,
                            base_labels.clone(),
                        ));
                    }

                    // Memory. Emitted per figure, only where the device
                    // reported one: a gauge states that the value is currently
                    // this, so exporting `0` for an adapter that reported no
                    // memory publishes an empty card as a fact. Prometheus reads
                    // an absent series as "not reported", which is true.
                    for (name, help, value) in [
                        (
                            "gpu_memory_total_bytes",
                            "GPU total memory in bytes",
                            info.dynamic_info.memory.total,
                        ),
                        (
                            "gpu_memory_used_bytes",
                            "GPU used memory in bytes",
                            info.dynamic_info.memory.used,
                        ),
                        (
                            "gpu_memory_free_bytes",
                            "GPU free memory in bytes",
                            info.dynamic_info.memory.free,
                        ),
                    ] {
                        if let Some(bytes) = value {
                            self.add(MetricFamily::gauge_with_labels(
                                &self.prefixed(name),
                                help,
                                bytes as f64,
                                base_labels.clone(),
                            ));
                        }
                    }

                    // Clocks
                    if let Some(graphics) = info.dynamic_info.clocks.graphics {
                        self.add(MetricFamily::gauge_with_labels(
                            // `graphics`, not `core`. NVML calls this the
                            // graphics clock, the ontology publishes it as
                            // `gpu.{n}.clocks.graphics`, and every bundled
                            // Grafana dashboard queries
                            // `ironmon_gpu_clock_graphics_mhz` -- so the one name
                            // that matched nothing was this one, and the panel
                            // was empty against a live server.
                            &self.prefixed("gpu_clock_graphics_mhz"),
                            "GPU graphics clock in MHz",
                            graphics as f64,
                            base_labels.clone(),
                        ));
                    }
                    if let Some(mem_clk) = info.dynamic_info.clocks.memory {
                        self.add(MetricFamily::gauge_with_labels(
                            &self.prefixed("gpu_clock_memory_mhz"),
                            "GPU memory clock in MHz",
                            mem_clk as f64,
                            base_labels.clone(),
                        ));
                    }

                    // Fan
                    if let Some(fan_speed) = info.dynamic_info.thermal.fan_speed {
                        self.add(MetricFamily::gauge_with_labels(
                            &self.prefixed("gpu_fan_speed_percent"),
                            "GPU fan speed percentage",
                            fan_speed as f64,
                            base_labels.clone(),
                        ));
                    }
                }
            }
        }
    }

    fn collect_disk_metrics(&mut self) {
        // One batched read for the whole machine rather than a WMI connection
        // per disk inside the loop: 9.9 ms against 8.0 s on a four-drive host,
        // paid on every scrape. See `crate::disk::all_io_counters`.
        let io_counters = crate::disk::all_io_counters();

        if let Ok(disks) = crate::disk::enumerate_disks() {
            for disk in &disks {
                if let Ok(filesystems) = disk.filesystem_info() {
                    for fs in &filesystems {
                        let mut labels = BTreeMap::new();
                        labels.insert("device".into(), disk.name().to_string());
                        labels.insert("mount".into(), fs.mount_point.to_string_lossy().to_string());

                        self.add(MetricFamily::gauge_with_labels(
                            &self.prefixed("disk_total_bytes"),
                            "Total disk capacity in bytes",
                            fs.total_size as f64,
                            labels.clone(),
                        ));
                        self.add(MetricFamily::gauge_with_labels(
                            &self.prefixed("disk_used_bytes"),
                            "Used disk space in bytes",
                            fs.used_size as f64,
                            labels.clone(),
                        ));
                        self.add(MetricFamily::gauge_with_labels(
                            &self.prefixed("disk_available_bytes"),
                            "Available disk space in bytes",
                            fs.available_size as f64,
                            labels.clone(),
                        ));
                        if fs.total_size > 0 {
                            self.add(MetricFamily::gauge_with_labels(
                                &self.prefixed("disk_usage_percent"),
                                "Disk utilization percentage",
                                (fs.used_size as f64 / fs.total_size as f64) * 100.0,
                                labels.clone(),
                            ));
                        }
                    }
                }

                // Cumulative I/O, queried by the bundled dashboards and never
                // published. These are counters -- `ae625ab` moved the Windows
                // reader to `Win32_PerfRawData_*` so that they are genuinely
                // cumulative rather than the instantaneous rates the class name
                // suggests -- so `rate()` over them in a dashboard is correct.
                if let Some(io) = io_counters.get(disk.name()) {
                    let mut labels = BTreeMap::new();
                    labels.insert("device".into(), disk.name().to_string());
                    self.add(MetricFamily::counter_with_labels(
                        &self.prefixed("disk_read_bytes_total"),
                        "Total bytes read from this device since boot",
                        io.read_bytes as f64,
                        labels.clone(),
                    ));
                    self.add(MetricFamily::counter_with_labels(
                        &self.prefixed("disk_write_bytes_total"),
                        "Total bytes written to this device since boot",
                        io.write_bytes as f64,
                        labels.clone(),
                    ));
                }
            }
        }
    }

    fn collect_network_metrics(&mut self) {
        if let Ok(mut monitor) = crate::NetworkMonitor::new() {
            if let Ok(interfaces) = monitor.interfaces() {
                for iface in &interfaces {
                    let mut labels = BTreeMap::new();
                    labels.insert("interface".into(), iface.name.clone());

                    self.add(MetricFamily::counter_with_labels(
                        &self.prefixed("network_rx_bytes_total"),
                        "Total bytes received",
                        iface.rx_bytes as f64,
                        labels.clone(),
                    ));
                    // The transmit total was never exported at all: the receive
                    // counter had no counterpart, so a dashboard could plot half
                    // of every link.
                    self.add(MetricFamily::counter_with_labels(
                        &self.prefixed("network_tx_bytes_total"),
                        "Total bytes transmitted",
                        iface.tx_bytes as f64,
                        labels.clone(),
                    ));

                    // Use gauges for current rates.
                    //
                    // Emitted only once there is a rate to emit. A gauge states
                    // that the value is currently this, so exporting `0` for an
                    // interface whose rate has not been established publishes an
                    // idle link as a fact; Prometheus already treats an absent
                    // series as "not reported", which is what is true. The first
                    // scrape after start-up has no baseline and omits these two.
                    let Some((rx_rate, tx_rate)) = monitor.bandwidth_rate(&iface.name, iface)
                    else {
                        continue;
                    };
                    self.add(MetricFamily::gauge_with_labels(
                        &self.prefixed("network_rx_bytes_per_sec"),
                        "Network receive rate in bytes per second",
                        rx_rate,
                        labels.clone(),
                    ));
                    self.add(MetricFamily::gauge_with_labels(
                        &self.prefixed("network_tx_bytes_per_sec"),
                        "Network transmit rate in bytes per second",
                        tx_rate,
                        labels.clone(),
                    ));
                }
            }
        }
    }

    /// Export all metrics in Prometheus text exposition format
    pub fn export(&self) -> String {
        let mut output = String::with_capacity(4096);

        // Add metadata comment
        output.push_str(&format!(
            "# IronMonitor v{} Prometheus Metrics\n\n",
            crate::VERSION
        ));

        for family in &self.families {
            output.push_str(&family.format());
            output.push('\n');
        }

        output
    }

    /// Export metrics and clear collected data
    pub fn export_and_clear(&mut self) -> String {
        let output = self.export();
        self.families.clear();
        output
    }

    /// Get the content type for Prometheus exposition format
    pub fn content_type() -> &'static str {
        "text/plain; version=0.0.4; charset=utf-8"
    }
}

/// Sanitize a string for use as a Prometheus metric name
fn sanitize_metric_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Escape a label value for Prometheus format
fn escape_label_value(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

/// Format a float value for Prometheus (special handling for NaN, Inf)
fn format_value(value: f64) -> String {
    if value.is_nan() {
        "NaN".to_string()
    } else if value.is_infinite() {
        if value.is_sign_positive() {
            "+Inf".to_string()
        } else {
            "-Inf".to_string()
        }
    } else if value == value.floor() && value.abs() < 1e15 {
        format!("{:.0}", value)
    } else {
        format!("{}", value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gauge_format() {
        let family = MetricFamily::gauge("test_metric", "A test metric", 42.5);
        let output = family.format();
        assert!(output.contains("# HELP test_metric A test metric"));
        assert!(output.contains("# TYPE test_metric gauge"));
        assert!(output.contains("test_metric 42.5"));
    }

    #[test]
    fn test_labeled_metric() {
        let mut labels = BTreeMap::new();
        labels.insert("gpu".into(), "0".into());
        labels.insert("name".into(), "RTX 4090".into());

        let family = MetricFamily::gauge_with_labels("gpu_temp", "GPU temperature", 72.0, labels);
        let output = family.format();
        assert!(output.contains("gpu=\"0\""));
        assert!(output.contains("name=\"RTX 4090\""));
        assert!(output.contains("72"));
    }

    #[test]
    fn test_sanitize_metric_name() {
        assert_eq!(
            sanitize_metric_name("cpu.usage-percent"),
            "cpu_usage_percent"
        );
        assert_eq!(sanitize_metric_name("valid_name"), "valid_name");
    }

    #[test]
    fn test_escape_label_value() {
        assert_eq!(escape_label_value("hello"), "hello");
        assert_eq!(escape_label_value("hello\"world"), "hello\\\"world");
        assert_eq!(escape_label_value("line\nnewline"), "line\\nnewline");
    }

    #[test]
    fn test_format_value() {
        assert_eq!(format_value(42.0), "42");
        assert_eq!(format_value(42.5), "42.5");
        assert_eq!(format_value(f64::NAN), "NaN");
        assert_eq!(format_value(f64::INFINITY), "+Inf");
    }

    #[test]
    fn test_exporter_collect_and_export() {
        let mut exporter = PrometheusExporter::new("test");
        exporter.add(MetricFamily::gauge("test_metric", "Help", 1.0));
        let output = exporter.export();
        assert!(output.contains("# HELP test_metric"));
        assert!(output.contains("test_metric 1"));
    }
}
