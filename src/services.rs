//! System Service Monitoring and Control
//!
//! This module provides cross-platform system service monitoring capabilities
//! including service status, management, and auto-start configuration.
//!
//! # Example
//!
//! ```no_run
//! use ironmonlib::services::{ServiceMonitor, ServiceStatus};
//!
//! let monitor = ServiceMonitor::new().unwrap();
//!
//! // List all services
//! for service in monitor.services() {
//!     println!("{}: {:?}", service.name, service.status);
//! }
//!
//! // Check specific service
//! if let Some(status) = monitor.get_service("sshd") {
//!     println!("SSH: {:?}", status.status);
//! }
//! ```

use crate::error::{IronError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Instant;

/// Service status states
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ServiceStatus {
    /// Service is running
    Running,
    /// Service is stopped
    Stopped,
    /// Service is starting
    Starting,
    /// Service is stopping
    Stopping,
    /// Service has failed
    Failed,
    /// Service status is unknown
    Unknown,
    /// Service is not found
    NotFound,
}

impl std::fmt::Display for ServiceStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServiceStatus::Running => write!(f, "running"),
            ServiceStatus::Stopped => write!(f, "stopped"),
            ServiceStatus::Starting => write!(f, "starting"),
            ServiceStatus::Stopping => write!(f, "stopping"),
            ServiceStatus::Failed => write!(f, "failed"),
            ServiceStatus::Unknown => write!(f, "unknown"),
            ServiceStatus::NotFound => write!(f, "not-found"),
        }
    }
}

/// Service type classification
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ServiceType {
    /// Simple foreground service
    Simple,
    /// Forking daemon
    Forking,
    /// Oneshot service
    Oneshot,
    /// D-Bus activated service
    Dbus,
    /// Notify service
    Notify,
    /// Idle service
    Idle,
    /// Windows service
    Win32,
    /// Unknown type
    Unknown,
}

/// Service startup type
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StartupType {
    /// Automatically start on boot
    Automatic,
    /// Manual start required
    Manual,
    /// Disabled
    Disabled,
    /// Start on demand
    OnDemand,
    /// Unknown
    Unknown,
}

impl std::fmt::Display for StartupType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StartupType::Automatic => write!(f, "automatic"),
            StartupType::Manual => write!(f, "manual"),
            StartupType::Disabled => write!(f, "disabled"),
            StartupType::OnDemand => write!(f, "on-demand"),
            StartupType::Unknown => write!(f, "unknown"),
        }
    }
}

/// Detailed information about a system service
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInfo {
    /// Service name
    pub name: String,
    /// Display name / description
    pub display_name: Option<String>,
    /// Current status
    pub status: ServiceStatus,
    /// Service type
    pub service_type: ServiceType,
    /// Startup type
    pub startup_type: StartupType,
    /// Process ID (if running)
    pub pid: Option<u32>,
    /// Memory usage (bytes)
    pub memory_bytes: Option<u64>,
    /// CPU time (seconds)
    pub cpu_time_secs: Option<f64>,
    /// Main process exit code
    pub exit_code: Option<i32>,
    /// Unit file path (Linux)
    pub unit_file: Option<String>,
    /// Service dependencies
    pub dependencies: Vec<String>,
    /// Services that depend on this
    pub dependents: Vec<String>,
    /// Is service enabled at boot
    pub enabled: bool,
    /// Load state (systemd)
    pub load_state: Option<String>,
    /// Sub-state (systemd)
    pub sub_state: Option<String>,
    /// Last error message
    pub error_message: Option<String>,
    /// Start time
    pub start_time: Option<String>,
}

impl ServiceInfo {
    /// Create a new ServiceInfo
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            display_name: None,
            status: ServiceStatus::Unknown,
            service_type: ServiceType::Unknown,
            startup_type: StartupType::Unknown,
            pid: None,
            memory_bytes: None,
            cpu_time_secs: None,
            exit_code: None,
            unit_file: None,
            dependencies: Vec::new(),
            dependents: Vec::new(),
            enabled: false,
            load_state: None,
            sub_state: None,
            error_message: None,
            start_time: None,
        }
    }

    /// Check if service is active
    pub fn is_active(&self) -> bool {
        self.status == ServiceStatus::Running
    }

    /// Check if service is failed
    pub fn is_failed(&self) -> bool {
        self.status == ServiceStatus::Failed
    }
}

/// System service monitor
pub struct ServiceMonitor {
    /// Discovered services
    services: Vec<ServiceInfo>,
    /// Service map by name
    service_map: HashMap<String, usize>,
    /// Last update time
    last_update: Instant,
    /// Filter for specific services
    filter: Option<Vec<String>>,
}

impl ServiceMonitor {
    /// Create a new service monitor
    pub fn new() -> Result<Self> {
        let mut monitor = Self {
            services: Vec::new(),
            service_map: HashMap::new(),
            last_update: Instant::now(),
            filter: None,
        };

        monitor.discover()?;
        Ok(monitor)
    }

    /// Create a monitor for specific services
    pub fn with_filter(service_names: Vec<String>) -> Result<Self> {
        let mut monitor = Self {
            services: Vec::new(),
            service_map: HashMap::new(),
            last_update: Instant::now(),
            filter: Some(service_names),
        };

        monitor.discover()?;
        Ok(monitor)
    }

    /// Refresh service information
    pub fn refresh(&mut self) -> Result<()> {
        self.services.clear();
        self.service_map.clear();
        self.discover()?;
        self.last_update = Instant::now();
        Ok(())
    }

    /// Get all services
    pub fn services(&self) -> &[ServiceInfo] {
        &self.services
    }

    /// Get service by name
    pub fn get_service(&self, name: &str) -> Option<&ServiceInfo> {
        self.service_map
            .get(name)
            .and_then(|&idx| self.services.get(idx))
    }

    /// Get running services
    pub fn running_services(&self) -> Vec<&ServiceInfo> {
        self.services
            .iter()
            .filter(|s| s.status == ServiceStatus::Running)
            .collect()
    }

    /// Get failed services
    pub fn failed_services(&self) -> Vec<&ServiceInfo> {
        self.services
            .iter()
            .filter(|s| s.status == ServiceStatus::Failed)
            .collect()
    }

    /// Check if a service is active
    pub fn is_active(&self, name: &str) -> bool {
        self.get_service(name)
            .map(|s| s.is_active())
            .unwrap_or(false)
    }

    /// Start a service
    pub fn start(&self, name: &str) -> Result<()> {
        #[cfg(target_os = "linux")]
        {
            self.linux_start_service(name)
        }

        #[cfg(target_os = "windows")]
        {
            self.windows_start_service(name)
        }

        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            let _ = name;
            Err(IronError::UnsupportedPlatform(
                "Service control not supported".to_string(),
            ))
        }
    }

    /// Stop a service
    pub fn stop(&self, name: &str) -> Result<()> {
        #[cfg(target_os = "linux")]
        {
            self.linux_stop_service(name)
        }

        #[cfg(target_os = "windows")]
        {
            self.windows_stop_service(name)
        }

        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            let _ = name;
            Err(IronError::UnsupportedPlatform(
                "Service control not supported".to_string(),
            ))
        }
    }

    /// Restart a service
    pub fn restart(&self, name: &str) -> Result<()> {
        #[cfg(target_os = "linux")]
        {
            self.linux_restart_service(name)
        }

        #[cfg(target_os = "windows")]
        {
            // Windows: stop then start
            self.windows_stop_service(name)?;
            std::thread::sleep(std::time::Duration::from_millis(500));
            self.windows_start_service(name)
        }

        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            let _ = name;
            Err(IronError::UnsupportedPlatform(
                "Service control not supported".to_string(),
            ))
        }
    }

    /// Enable service at boot
    pub fn enable(&self, name: &str) -> Result<()> {
        #[cfg(target_os = "linux")]
        {
            self.linux_enable_service(name)
        }

        #[cfg(target_os = "windows")]
        {
            self.windows_set_startup(name, StartupType::Automatic)
        }

        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            let _ = name;
            Err(IronError::UnsupportedPlatform(
                "Service control not supported".to_string(),
            ))
        }
    }

    /// Disable service at boot
    pub fn disable(&self, name: &str) -> Result<()> {
        #[cfg(target_os = "linux")]
        {
            self.linux_disable_service(name)
        }

        #[cfg(target_os = "windows")]
        {
            self.windows_set_startup(name, StartupType::Manual)
        }

        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            let _ = name;
            Err(IronError::UnsupportedPlatform(
                "Service control not supported".to_string(),
            ))
        }
    }

    // Platform discovery
    fn discover(&mut self) -> Result<()> {
        #[cfg(target_os = "linux")]
        {
            self.linux_discover()
        }

        #[cfg(target_os = "windows")]
        {
            self.windows_discover()
        }

        #[cfg(target_os = "macos")]
        {
            self.macos_discover()
        }

        #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
        {
            Ok(())
        }
    }

    // ==================== Linux Implementation ====================

    #[cfg(target_os = "linux")]
    fn linux_discover(&mut self) -> Result<()> {
        use std::process::Command;

        // Use systemctl to list services
        let output = Command::new("systemctl")
            .args([
                "list-units",
                "--type=service",
                "--all",
                "--no-pager",
                "--no-legend",
            ])
            .output();

        if let Ok(output) = output {
            if output.status.success() {
                if let Ok(stdout) = String::from_utf8(output.stdout) {
                    for line in stdout.lines() {
                        if let Some(service) = self.linux_parse_service_line(line) {
                            // Apply filter if set
                            if let Some(ref filter) = self.filter {
                                if !filter.iter().any(|f| service.name.contains(f)) {
                                    continue;
                                }
                            }

                            let idx = self.services.len();
                            self.service_map.insert(service.name.clone(), idx);
                            self.services.push(service);
                        }
                    }
                }
            }
        }

        // Get detailed info for each service.
        //
        // Iterating `&mut self.services` while calling `&self` method borrows self
        // both ways at once. Move the vector out for the duration and put it back:
        // `linux_get_service_details` does not read `self.services`, so nothing
        // observes it missing.
        let mut services = std::mem::take(&mut self.services);
        for service in &mut services {
            self.linux_get_service_details(service);
        }
        self.services = services;

        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn linux_parse_service_line(&self, line: &str) -> Option<ServiceInfo> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 4 {
            return None;
        }

        let name = parts[0].trim_end_matches(".service").to_string();
        let load_state = parts[1].to_string();
        let active_state = parts[2].to_string();
        let sub_state = parts[3].to_string();

        let status = match active_state.as_str() {
            "active" => match sub_state.as_str() {
                "running" => ServiceStatus::Running,
                "exited" => ServiceStatus::Stopped,
                "waiting" => ServiceStatus::Starting,
                _ => ServiceStatus::Running,
            },
            "inactive" => ServiceStatus::Stopped,
            "failed" => ServiceStatus::Failed,
            "activating" => ServiceStatus::Starting,
            "deactivating" => ServiceStatus::Stopping,
            _ => ServiceStatus::Unknown,
        };

        let mut service = ServiceInfo::new(&name);
        service.status = status;
        service.load_state = Some(load_state);
        service.sub_state = Some(sub_state);

        // Get description from remaining parts
        if parts.len() > 4 {
            service.display_name = Some(parts[4..].join(" "));
        }

        Some(service)
    }

    #[cfg(target_os = "linux")]
    fn linux_get_service_details(&self, service: &mut ServiceInfo) {
        use std::process::Command;

        // Get detailed service info
        let output = Command::new("systemctl")
            .args(["show", &format!("{}.service", service.name)])
            .output();

        if let Ok(output) = output {
            if let Ok(stdout) = String::from_utf8(output.stdout) {
                for line in stdout.lines() {
                    if let Some((key, value)) = line.split_once('=') {
                        match key {
                            "MainPID" => {
                                if let Ok(pid) = value.parse::<u32>() {
                                    if pid > 0 {
                                        service.pid = Some(pid);
                                    }
                                }
                            }
                            "MemoryCurrent" => {
                                if let Ok(mem) = value.parse::<u64>() {
                                    if mem < u64::MAX {
                                        service.memory_bytes = Some(mem);
                                    }
                                }
                            }
                            "ExecMainExitTimestamp" => {
                                if !value.is_empty() {
                                    service.start_time = Some(value.to_string());
                                }
                            }
                            "Type" => {
                                service.service_type = match value {
                                    "simple" => ServiceType::Simple,
                                    "forking" => ServiceType::Forking,
                                    "oneshot" => ServiceType::Oneshot,
                                    "dbus" => ServiceType::Dbus,
                                    "notify" => ServiceType::Notify,
                                    "idle" => ServiceType::Idle,
                                    _ => ServiceType::Unknown,
                                };
                            }
                            "UnitFileState" => {
                                service.enabled = value == "enabled";
                                service.startup_type = match value {
                                    "enabled" => StartupType::Automatic,
                                    "disabled" => StartupType::Disabled,
                                    "static" => StartupType::Manual,
                                    "masked" => StartupType::Disabled,
                                    _ => StartupType::Unknown,
                                };
                            }
                            "FragmentPath" => {
                                if !value.is_empty() {
                                    service.unit_file = Some(value.to_string());
                                }
                            }
                            "Requires" | "Wants" => {
                                for dep in value.split_whitespace() {
                                    let dep_name = dep.trim_end_matches(".service");
                                    if !dep_name.is_empty() {
                                        service.dependencies.push(dep_name.to_string());
                                    }
                                }
                            }
                            "RequiredBy" | "WantedBy" => {
                                for dep in value.split_whitespace() {
                                    let dep_name = dep.trim_end_matches(".service");
                                    if !dep_name.is_empty() {
                                        service.dependents.push(dep_name.to_string());
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    fn linux_start_service(&self, name: &str) -> Result<()> {
        use std::process::Command;

        let status = Command::new("systemctl")
            .args(["start", &format!("{}.service", name)])
            .status();

        match status {
            Ok(s) if s.success() => Ok(()),
            Ok(_) => Err(IronError::System(format!(
                "Failed to start service '{}' (check permissions)",
                name
            ))),
            Err(e) => Err(IronError::System(format!(
                "Failed to execute systemctl: {}",
                e
            ))),
        }
    }

    #[cfg(target_os = "linux")]
    fn linux_stop_service(&self, name: &str) -> Result<()> {
        use std::process::Command;

        let status = Command::new("systemctl")
            .args(["stop", &format!("{}.service", name)])
            .status();

        match status {
            Ok(s) if s.success() => Ok(()),
            Ok(_) => Err(IronError::System(format!(
                "Failed to stop service '{}' (check permissions)",
                name
            ))),
            Err(e) => Err(IronError::System(format!(
                "Failed to execute systemctl: {}",
                e
            ))),
        }
    }

    #[cfg(target_os = "linux")]
    fn linux_restart_service(&self, name: &str) -> Result<()> {
        use std::process::Command;

        let status = Command::new("systemctl")
            .args(["restart", &format!("{}.service", name)])
            .status();

        match status {
            Ok(s) if s.success() => Ok(()),
            Ok(_) => Err(IronError::System(format!(
                "Failed to restart service '{}' (check permissions)",
                name
            ))),
            Err(e) => Err(IronError::System(format!(
                "Failed to execute systemctl: {}",
                e
            ))),
        }
    }

    #[cfg(target_os = "linux")]
    fn linux_enable_service(&self, name: &str) -> Result<()> {
        use std::process::Command;

        let status = Command::new("systemctl")
            .args(["enable", &format!("{}.service", name)])
            .status();

        match status {
            Ok(s) if s.success() => Ok(()),
            Ok(_) => Err(IronError::System(format!(
                "Failed to enable service '{}' (check permissions)",
                name
            ))),
            Err(e) => Err(IronError::System(format!(
                "Failed to execute systemctl: {}",
                e
            ))),
        }
    }

    #[cfg(target_os = "linux")]
    fn linux_disable_service(&self, name: &str) -> Result<()> {
        use std::process::Command;

        let status = Command::new("systemctl")
            .args(["disable", &format!("{}.service", name)])
            .status();

        match status {
            Ok(s) if s.success() => Ok(()),
            Ok(_) => Err(IronError::System(format!(
                "Failed to disable service '{}' (check permissions)",
                name
            ))),
            Err(e) => Err(IronError::System(format!(
                "Failed to execute systemctl: {}",
                e
            ))),
        }
    }

    // ==================== Windows Implementation ====================

    #[cfg(target_os = "windows")]
    fn windows_discover(&mut self) -> Result<()> {
        use std::process::Command;

        // Use PowerShell to get services
        let output = Command::new("powershell")
            .args([
                "-Command",
                "Get-Service | Select-Object Name, DisplayName, Status, StartType | ConvertTo-Json",
            ])
            .output();

        if let Ok(output) = output {
            if output.status.success() {
                if let Ok(stdout) = String::from_utf8(output.stdout) {
                    self.windows_parse_services(&stdout);
                }
            }
        }

        Ok(())
    }

    #[cfg(target_os = "windows")]
    fn windows_parse_services(&mut self, json_str: &str) {
        // Parse PowerShell JSON output
        #[derive(serde::Deserialize)]
        struct PsService {
            #[serde(rename = "Name")]
            name: Option<String>,
            #[serde(rename = "DisplayName")]
            display_name: Option<String>,
            #[serde(rename = "Status")]
            status: Option<i32>,
            #[serde(rename = "StartType")]
            start_type: Option<i32>,
        }

        // Try to parse as array or single object
        if let Ok(services) = serde_json::from_str::<Vec<PsService>>(json_str) {
            for ps_svc in services {
                if let Some(name) = ps_svc.name {
                    // Apply filter if set
                    if let Some(ref filter) = self.filter {
                        if !filter.iter().any(|f| name.contains(f)) {
                            continue;
                        }
                    }

                    let mut service = ServiceInfo::new(&name);
                    service.display_name = ps_svc.display_name;
                    service.service_type = ServiceType::Win32;

                    // Status: 1=Stopped, 2=StartPending, 3=StopPending, 4=Running
                    service.status = match ps_svc.status {
                        Some(1) => ServiceStatus::Stopped,
                        Some(2) => ServiceStatus::Starting,
                        Some(3) => ServiceStatus::Stopping,
                        Some(4) => ServiceStatus::Running,
                        _ => ServiceStatus::Unknown,
                    };

                    // StartType: 0=Boot, 1=System, 2=Automatic, 3=Manual, 4=Disabled
                    service.startup_type = match ps_svc.start_type {
                        Some(0) | Some(1) | Some(2) => StartupType::Automatic,
                        Some(3) => StartupType::Manual,
                        Some(4) => StartupType::Disabled,
                        _ => StartupType::Unknown,
                    };

                    service.enabled = matches!(ps_svc.start_type, Some(0) | Some(1) | Some(2));

                    let idx = self.services.len();
                    self.service_map.insert(name, idx);
                    self.services.push(service);
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    fn windows_start_service(&self, name: &str) -> Result<()> {
        use std::process::Command;

        let status = Command::new("sc").args(["start", name]).status();

        match status {
            Ok(s) if s.success() => Ok(()),
            Ok(_) => Err(IronError::System(format!(
                "Failed to start service '{}' (run as Administrator)",
                name
            ))),
            Err(e) => Err(IronError::System(format!("Failed to execute sc: {}", e))),
        }
    }

    #[cfg(target_os = "windows")]
    fn windows_stop_service(&self, name: &str) -> Result<()> {
        use std::process::Command;

        let status = Command::new("sc").args(["stop", name]).status();

        match status {
            Ok(s) if s.success() => Ok(()),
            Ok(_) => Err(IronError::System(format!(
                "Failed to stop service '{}' (run as Administrator)",
                name
            ))),
            Err(e) => Err(IronError::System(format!("Failed to execute sc: {}", e))),
        }
    }

    #[cfg(target_os = "windows")]
    fn windows_set_startup(&self, name: &str, startup: StartupType) -> Result<()> {
        use std::process::Command;

        let start_type = match startup {
            StartupType::Automatic => "auto",
            StartupType::Manual => "demand",
            StartupType::Disabled => "disabled",
            _ => "demand",
        };

        let status = Command::new("sc")
            .args(["config", name, "start=", start_type])
            .status();

        match status {
            Ok(s) if s.success() => Ok(()),
            Ok(_) => Err(IronError::System(format!(
                "Failed to configure service '{}' (run as Administrator)",
                name
            ))),
            Err(e) => Err(IronError::System(format!("Failed to execute sc: {}", e))),
        }
    }

    // ==================== macOS Implementation ====================

    #[cfg(target_os = "macos")]
    fn macos_discover(&mut self) -> Result<()> {
        use std::process::Command;

        // Use launchctl to list services
        let output = Command::new("launchctl").args(["list"]).output();

        if let Ok(output) = output {
            if output.status.success() {
                if let Ok(stdout) = String::from_utf8(output.stdout) {
                    for line in stdout.lines().skip(1) {
                        // Skip header
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        if parts.len() >= 3 {
                            let name = parts[2].to_string();

                            // Apply filter if set
                            if let Some(ref filter) = self.filter {
                                if !filter.iter().any(|f| name.contains(f)) {
                                    continue;
                                }
                            }

                            let mut service = ServiceInfo::new(&name);

                            // PID column
                            if let Ok(pid) = parts[0].parse::<u32>() {
                                service.pid = Some(pid);
                                service.status = ServiceStatus::Running;
                            } else if parts[0] == "-" {
                                service.status = ServiceStatus::Stopped;
                            }

                            // Exit code column
                            if let Ok(code) = parts[1].parse::<i32>() {
                                service.exit_code = Some(code);
                                if code != 0 && service.status == ServiceStatus::Stopped {
                                    service.status = ServiceStatus::Failed;
                                }
                            }

                            let idx = self.services.len();
                            self.service_map.insert(name, idx);
                            self.services.push(service);
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

/// Service summary for quick overview
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceSummary {
    /// Total services
    pub total: usize,
    /// Running services
    pub running: usize,
    /// Stopped services
    pub stopped: usize,
    /// Failed services
    pub failed: usize,
    /// Enabled at boot
    pub enabled: usize,
}

/// Get a quick summary of services
pub fn service_summary() -> Result<ServiceSummary> {
    let monitor = ServiceMonitor::new()?;
    let services = monitor.services();

    Ok(ServiceSummary {
        total: services.len(),
        running: services
            .iter()
            .filter(|s| s.status == ServiceStatus::Running)
            .count(),
        stopped: services
            .iter()
            .filter(|s| s.status == ServiceStatus::Stopped)
            .count(),
        failed: services
            .iter()
            .filter(|s| s.status == ServiceStatus::Failed)
            .count(),
        enabled: services.iter().filter(|s| s.enabled).count(),
    })
}

/// Check if a specific service is running
pub fn is_service_running(name: &str) -> bool {
    if let Ok(monitor) = ServiceMonitor::with_filter(vec![name.to_string()]) {
        monitor.is_active(name)
    } else {
        false
    }
}

/// Get status of specific services
pub fn get_services_status(names: Vec<&str>) -> Result<HashMap<String, ServiceStatus>> {
    let filter: Vec<String> = names.iter().map(|s| s.to_string()).collect();
    let monitor = ServiceMonitor::with_filter(filter)?;

    let mut statuses = HashMap::new();
    for name in names {
        let status = monitor
            .get_service(name)
            .map(|s| s.status.clone())
            .unwrap_or(ServiceStatus::NotFound);
        statuses.insert(name.to_string(), status);
    }

    Ok(statuses)
}

/// Common system services to monitor
pub fn common_services() -> Vec<&'static str> {
    #[cfg(target_os = "linux")]
    {
        vec![
            "sshd",
            "docker",
            "nginx",
            "apache2",
            "postgresql",
            "mysql",
            "redis",
            "mongodb",
            "NetworkManager",
            "bluetooth",
            "cups",
            "cron",
            "rsyslog",
            "ufw",
        ]
    }

    #[cfg(target_os = "windows")]
    {
        vec![
            "wuauserv",          // Windows Update
            "BITS",              // Background Intelligent Transfer
            "Spooler",           // Print Spooler
            "WinDefend",         // Windows Defender
            "Dhcp",              // DHCP Client
            "Dnscache",          // DNS Client
            "EventLog",          // Windows Event Log
            "W32Time",           // Windows Time
            "AudioSrv",          // Windows Audio
            "LanmanServer",      // Server (file sharing)
            "LanmanWorkstation", // Workstation
            "MpsSvc",            // Windows Firewall
        ]
    }

    #[cfg(target_os = "macos")]
    {
        vec![
            "com.apple.dock",
            "com.apple.Finder",
            "com.apple.loginwindow",
            "com.apple.WindowServer",
            "com.apple.SystemUIServer",
            "com.apple.wifi.WiFiAgent",
            "com.apple.bluetoothd",
        ]
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_info_new_defaults() {
        let info = ServiceInfo::new("sshd");
        assert_eq!(info.name, "sshd");
        assert_eq!(info.status, ServiceStatus::Unknown);
        assert!(!info.enabled);
        assert!(info.dependencies.is_empty());
        assert!(info.dependents.is_empty());
        assert!(info.pid.is_none());
    }

    #[test]
    fn test_service_info_is_active() {
        let mut info = ServiceInfo::new("test");
        assert!(!info.is_active());
        info.status = ServiceStatus::Running;
        assert!(info.is_active());
        info.status = ServiceStatus::Stopped;
        assert!(!info.is_active());
    }

    #[test]
    fn test_service_info_is_failed() {
        let mut info = ServiceInfo::new("test");
        assert!(!info.is_failed());
        info.status = ServiceStatus::Failed;
        assert!(info.is_failed());
    }

    #[test]
    fn test_service_status_display() {
        assert_eq!(ServiceStatus::Running.to_string(), "running");
        assert_eq!(ServiceStatus::Stopped.to_string(), "stopped");
        assert_eq!(ServiceStatus::Starting.to_string(), "starting");
        assert_eq!(ServiceStatus::Stopping.to_string(), "stopping");
        assert_eq!(ServiceStatus::Failed.to_string(), "failed");
        assert_eq!(ServiceStatus::Unknown.to_string(), "unknown");
        assert_eq!(ServiceStatus::NotFound.to_string(), "not-found");
    }

    #[test]
    fn test_startup_type_display() {
        assert_eq!(StartupType::Automatic.to_string(), "automatic");
        assert_eq!(StartupType::Manual.to_string(), "manual");
        assert_eq!(StartupType::Disabled.to_string(), "disabled");
        assert_eq!(StartupType::OnDemand.to_string(), "on-demand");
        assert_eq!(StartupType::Unknown.to_string(), "unknown");
    }

    #[test]
    fn test_service_info_serde_roundtrip() {
        let info = ServiceInfo::new("nginx");
        let json = serde_json::to_string(&info).unwrap();
        let parsed: ServiceInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "nginx");
        assert_eq!(parsed.status, ServiceStatus::Unknown);
    }
}
