// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2026 nervosys

//! Monitoring daemon for headless/remote operation
//!
//! Runs ironmon as a background service with HTTP API, Prometheus metrics,
//! and optional fleet push reporting.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DaemonError {
    #[error("Configuration error: {0}")]
    Config(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Already running (PID file exists): {0}")]
    AlreadyRunning(String),
}

/// Log level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum LogLevel {
    Error,
    Warn,
    #[default]
    Info,
    Debug,
    Trace,
}

/// Fleet push configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetPushConfig {
    pub enabled: bool,
    pub endpoint: String,
    pub host_id: String,
    pub interval_secs: u64,
    pub tags: std::collections::HashMap<String, String>,
}

/// Daemon configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    pub host: String,
    pub port: u16,
    pub poll_interval_secs: u64,
    pub pid_file: Option<String>,
    pub log_level: LogLevel,
    pub enable_prometheus: bool,
    pub enable_rest_api: bool,
    /// API key required for requests.
    ///
    /// Required whenever `host` is not a loopback address: the default bind is
    /// `0.0.0.0`, and serving hardware telemetry unauthenticated to every host that
    /// can reach the port is not a reasonable default. [`MonitoringDaemon::run`]
    /// refuses to start in that combination.
    #[serde(default)]
    pub api_key: Option<String>,
    pub fleet: Option<FleetPushConfig>,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".into(),
            port: 9100,
            poll_interval_secs: 5,
            pid_file: None,
            log_level: LogLevel::Info,
            enable_prometheus: true,
            enable_rest_api: true,
            api_key: None,
            fleet: None,
        }
    }
}

impl DaemonConfig {
    /// Load from TOML file
    pub fn from_toml_file(path: &str) -> Result<Self, DaemonError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| DaemonError::Config(format!("Cannot read {}: {}", path, e)))?;
        Self::from_toml(&content)
    }

    /// Parse from TOML string
    pub fn from_toml(content: &str) -> Result<Self, DaemonError> {
        toml::from_str(content).map_err(|e| DaemonError::Config(format!("TOML parse error: {}", e)))
    }

    /// Generate sample config
    pub fn sample_toml() -> String {
        r#"# IronMonitor Monitoring Daemon Configuration
host = "0.0.0.0"
port = 9100
poll_interval_secs = 5
# pid_file = "/var/run/ironmon.pid"
log_level = "Info"
enable_prometheus = true
enable_rest_api = true

# Required unless `host` is a loopback address. The daemon refuses to start
# bound to a routable address without one, because doing so would serve full
# hardware telemetry to anything that can reach the port.
# api_key = "change-me"

# Optional: Fleet push reporting
# NOTE: not implemented. These keys parse, but nothing pushes to a fleet
# endpoint yet; the daemon warns at startup if `enabled` is true.
# [fleet]
# enabled = true
# endpoint = "http://fleet-server:9200/api/v1/metrics"
# host_id = "host-001"
# interval_secs = 30
# [fleet.tags]
# environment = "production"
# datacenter = "us-east-1"
# rack = "rack-42"
"#
        .into()
    }
}

/// Monitoring daemon
pub struct MonitoringDaemon {
    config: DaemonConfig,
}

impl MonitoringDaemon {
    pub fn new(config: DaemonConfig) -> Self {
        Self { config }
    }

    /// Load from config file
    pub fn from_config_file(path: &str) -> Result<Self, DaemonError> {
        let config = DaemonConfig::from_toml_file(path)?;
        Ok(Self::new(config))
    }

    /// Get daemon configuration
    pub fn config(&self) -> &DaemonConfig {
        &self.config
    }

    /// Write PID file
    pub fn write_pid_file(&self) -> Result<(), DaemonError> {
        if let Some(ref pid_path) = self.config.pid_file {
            // Check if already running
            if std::path::Path::new(pid_path).exists() {
                let existing = std::fs::read_to_string(pid_path).unwrap_or_default();
                if !existing.trim().is_empty() {
                    return Err(DaemonError::AlreadyRunning(pid_path.clone()));
                }
            }
            let pid = std::process::id();
            std::fs::write(pid_path, pid.to_string())?;
        }
        Ok(())
    }

    /// Remove PID file
    pub fn remove_pid_file(&self) {
        if let Some(ref pid_path) = self.config.pid_file {
            let _ = std::fs::remove_file(pid_path);
        }
    }

    /// Get listen address
    pub fn listen_address(&self) -> String {
        format!("{}:{}", self.config.host, self.config.port)
    }

    /// Check if prometheus endpoint is enabled
    pub fn prometheus_enabled(&self) -> bool {
        self.config.enable_prometheus
    }

    /// Check if REST API is enabled
    pub fn rest_api_enabled(&self) -> bool {
        self.config.enable_rest_api
    }

    /// Check if fleet push is enabled
    pub fn fleet_push_enabled(&self) -> bool {
        self.config
            .fleet
            .as_ref()
            .map(|f| f.enabled)
            .unwrap_or(false)
    }

    /// Whether `host` refers to this machine only.
    fn binds_loopback(&self) -> bool {
        matches!(self.config.host.as_str(), "127.0.0.1" | "::1" | "localhost")
    }

    /// Run the daemon until the process is terminated.
    ///
    /// Until this existed, `MonitoringDaemon` was configuration plumbing with no way
    /// to run anything: it parsed TOML, wrote a PID file and exposed getters, but
    /// never started a server or collected a sample. `grafana/README.md` nonetheless
    /// documented `ironmon daemon --config ironmon.toml` as a way to serve metrics.
    ///
    /// Refuses to start when bound to a routable address without an API key. The
    /// default host is `0.0.0.0`, so without that check the common path would serve
    /// unauthenticated hardware telemetry to the whole network.
    /// Check the configuration is safe and coherent before anything starts.
    ///
    /// Separate from [`MonitoringDaemon::run`] so a caller can fail fast — and print
    /// an accurate startup banner — rather than announcing settings it is about to
    /// reject.
    pub fn validate(&self) -> Result<(), DaemonError> {
        if !self.binds_loopback() && self.config.api_key.is_none() {
            return Err(DaemonError::Config(format!(
                "host is {} but no api_key is set. Serving hardware telemetry \
                 unauthenticated on a routable address is refused; set api_key, or \
                 bind 127.0.0.1 for local-only access.",
                self.config.host
            )));
        }

        if !self.config.enable_rest_api && !self.config.enable_prometheus {
            return Err(DaemonError::Config(
                "both enable_rest_api and enable_prometheus are false, so the daemon \
                 would serve nothing"
                    .to_string(),
            ));
        }

        Ok(())
    }

    #[cfg(feature = "cli")]
    pub async fn run(&self) -> Result<(), DaemonError> {
        use crate::http_server::{HttpServer, HttpServerConfig};

        self.validate()?;

        if self.fleet_push_enabled() {
            // Say so rather than silently ignoring a configured feature.
            eprintln!(
                "[ironmon] warning: fleet push is configured but not implemented; \
                 no metrics will be pushed to the fleet endpoint"
            );
        }

        self.write_pid_file()?;

        let server = HttpServer::new(HttpServerConfig {
            bind_address: self.config.host.clone(),
            port: self.config.port,
            metric_interval_secs: self.config.poll_interval_secs.max(1),
            api_key: self.config.api_key.clone(),
            request_logging: matches!(self.config.log_level, LogLevel::Debug | LogLevel::Trace),
            ..Default::default()
        })
        .map_err(|e| DaemonError::Config(format!("failed to create HTTP server: {e}")))?;

        // The PID file is removed by `Drop`, so an error here still cleans up.
        server
            .run()
            .await
            .map_err(|e| DaemonError::Config(format!("server error: {e}")))
    }
}

impl Drop for MonitoringDaemon {
    fn drop(&mut self) {
        self.remove_pid_file();
    }
}
