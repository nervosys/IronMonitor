//! Permission System - MCP-like Capability-Based Access Control
//!
//! This module implements a permission system inspired by the Model Context Protocol (MCP)
//! for controlling access to system observability data.
//!
//! # Permission Model
//!
//! Permissions are structured hierarchically:
//! - **Capability**: Broad category (e.g., "gpu", "process", "network")
//! - **Scope**: Specific action within capability (e.g., "read", "control", "kill")
//! - **Resource**: Optional resource filter (e.g., "gpu:0", "process:1234")
//!
//! # Configuration
//!
//! Permissions are configured via TOML/JSON similar to MCP server configs:
//!
//! ```toml
//! [api]
//! enabled = true
//! port = 8080
//! require_auth = true
//!
//! [[api.keys]]
//! name = "ai-agent-1"
//! key = "sk-xxxx"
//! capabilities = ["gpu:read", "cpu:read", "memory:read"]
//! rate_limit = 100  # requests per minute
//!
//! [[api.keys]]
//! name = "admin"
//! key = "sk-admin"
//! capabilities = ["*"]  # Full access
//! ```

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// Capability categories for system access
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    // Hardware monitoring (read-only)
    /// GPU metrics and information
    Gpu,
    /// CPU metrics and information
    Cpu,
    /// Memory metrics and information
    Memory,
    /// Disk metrics and information
    Disk,
    /// Network metrics and information
    Network,
    /// Motherboard/hardware sensors
    Motherboard,
    /// Power supply information
    Power,
    /// Fan information
    Fan,

    // Process management
    /// Process listing and metrics
    Process,
    /// Connection/socket information
    Connections,
    /// Service status
    Services,

    // System information
    /// System identification info
    SystemInfo,
    /// Boot configuration
    BootConfig,
    /// Driver information
    Drivers,

    // Advanced features
    /// Historical data queries
    History,
    /// Real-time event streaming
    Events,
    /// System context materialization
    Context,

    // Control capabilities (dangerous)
    /// Fan control
    FanControl,
    /// Process control (kill, priority)
    ProcessControl,
    /// GPU control (clocks, power limits)
    GpuControl,
    /// System power control (shutdown, reboot)
    PowerControl,

    // Administrative
    /// API management
    Admin,
}

impl Capability {
    /// Check if this is a read-only capability
    pub fn is_read_only(&self) -> bool {
        matches!(
            self,
            Capability::Gpu
                | Capability::Cpu
                | Capability::Memory
                | Capability::Disk
                | Capability::Network
                | Capability::Motherboard
                | Capability::Power
                | Capability::Fan
                | Capability::Process
                | Capability::Connections
                | Capability::Services
                | Capability::SystemInfo
                | Capability::BootConfig
                | Capability::Drivers
                | Capability::History
                | Capability::Events
                | Capability::Context
        )
    }

    /// Check if this is a control capability (potentially dangerous)
    pub fn is_control(&self) -> bool {
        matches!(
            self,
            Capability::FanControl
                | Capability::ProcessControl
                | Capability::GpuControl
                | Capability::PowerControl
        )
    }

    /// Get all capabilities
    pub fn all() -> Vec<Capability> {
        vec![
            Capability::Gpu,
            Capability::Cpu,
            Capability::Memory,
            Capability::Disk,
            Capability::Network,
            Capability::Motherboard,
            Capability::Power,
            Capability::Fan,
            Capability::Process,
            Capability::Connections,
            Capability::Services,
            Capability::SystemInfo,
            Capability::BootConfig,
            Capability::Drivers,
            Capability::History,
            Capability::Events,
            Capability::Context,
            Capability::FanControl,
            Capability::ProcessControl,
            Capability::GpuControl,
            Capability::PowerControl,
            Capability::Admin,
        ]
    }

    /// Get read-only capabilities (safe for external AI)
    pub fn read_only_set() -> HashSet<Capability> {
        Capability::all()
            .into_iter()
            .filter(|c| c.is_read_only())
            .collect()
    }
}

impl std::fmt::Display for Capability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Capability::Gpu => "gpu",
            Capability::Cpu => "cpu",
            Capability::Memory => "memory",
            Capability::Disk => "disk",
            Capability::Network => "network",
            Capability::Motherboard => "motherboard",
            Capability::Power => "power",
            Capability::Fan => "fan",
            Capability::Process => "process",
            Capability::Connections => "connections",
            Capability::Services => "services",
            Capability::SystemInfo => "system_info",
            Capability::BootConfig => "boot_config",
            Capability::Drivers => "drivers",
            Capability::History => "history",
            Capability::Events => "events",
            Capability::Context => "context",
            Capability::FanControl => "fan_control",
            Capability::ProcessControl => "process_control",
            Capability::GpuControl => "gpu_control",
            Capability::PowerControl => "power_control",
            Capability::Admin => "admin",
        };
        write!(f, "{}", name)
    }
}

impl std::str::FromStr for Capability {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "gpu" => Ok(Capability::Gpu),
            "cpu" => Ok(Capability::Cpu),
            "memory" => Ok(Capability::Memory),
            "disk" => Ok(Capability::Disk),
            "network" => Ok(Capability::Network),
            "motherboard" => Ok(Capability::Motherboard),
            "power" => Ok(Capability::Power),
            "fan" => Ok(Capability::Fan),
            "process" => Ok(Capability::Process),
            "connections" => Ok(Capability::Connections),
            "services" => Ok(Capability::Services),
            "system_info" | "systeminfo" => Ok(Capability::SystemInfo),
            "boot_config" | "bootconfig" => Ok(Capability::BootConfig),
            "drivers" => Ok(Capability::Drivers),
            "history" => Ok(Capability::History),
            "events" => Ok(Capability::Events),
            "context" => Ok(Capability::Context),
            "fan_control" | "fancontrol" => Ok(Capability::FanControl),
            "process_control" | "processcontrol" => Ok(Capability::ProcessControl),
            "gpu_control" | "gpucontrol" => Ok(Capability::GpuControl),
            "power_control" | "powercontrol" => Ok(Capability::PowerControl),
            "admin" => Ok(Capability::Admin),
            "*" | "all" => Ok(Capability::Admin), // Admin grants all
            _ => Err(format!("Unknown capability: {}", s)),
        }
    }
}

/// Permission scope within a capability
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum Scope {
    /// Read access only
    #[default]
    Read,
    /// Write/control access
    Write,
    /// Full access
    All,
    /// Specific resource (e.g., "gpu:0", "process:1234")
    Resource(String),
}

/// A single permission grant
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Permission {
    /// Capability being granted
    pub capability: Capability,
    /// Scope of the permission
    pub scope: Scope,
    /// Optional resource filter
    pub resource: Option<String>,
}

impl Permission {
    /// Create a read permission for a capability
    pub fn read(capability: Capability) -> Self {
        Self {
            capability,
            scope: Scope::Read,
            resource: None,
        }
    }

    /// Create a full permission for a capability
    pub fn full(capability: Capability) -> Self {
        Self {
            capability,
            scope: Scope::All,
            resource: None,
        }
    }

    /// Create a permission for a specific resource
    pub fn resource(capability: Capability, resource: impl Into<String>) -> Self {
        let resource_str = resource.into();
        Self {
            capability,
            scope: Scope::Resource(resource_str.clone()),
            resource: Some(resource_str),
        }
    }

    /// Parse from string format "capability:scope" or just "capability"
    pub fn parse(s: &str) -> Result<Self, String> {
        let parts: Vec<&str> = s.split(':').collect();
        match parts.len() {
            1 => {
                let cap = parts[0].parse()?;
                Ok(Permission::read(cap))
            }
            2 => {
                let cap = parts[0].parse()?;
                let scope = match parts[1].to_lowercase().as_str() {
                    "read" | "r" => Scope::Read,
                    "write" | "w" => Scope::Write,
                    "all" | "*" => Scope::All,
                    resource => Scope::Resource(resource.to_string()),
                };
                Ok(Permission {
                    capability: cap,
                    scope,
                    resource: None,
                })
            }
            _ => Err(format!("Invalid permission format: {}", s)),
        }
    }
}

impl std::fmt::Display for Permission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.scope {
            Scope::Read => write!(f, "{}:read", self.capability),
            Scope::Write => write!(f, "{}:write", self.capability),
            Scope::All => write!(f, "{}:*", self.capability),
            Scope::Resource(r) => write!(f, "{}:{}", self.capability, r),
        }
    }
}

/// Rate limit configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimit {
    /// Maximum requests per period
    pub max_requests: u32,
    /// Period duration in seconds
    pub period_secs: u64,
}

impl RateLimit {
    /// Get period as Duration
    pub fn period(&self) -> Duration {
        Duration::from_secs(self.period_secs)
    }
}

impl Default for RateLimit {
    fn default() -> Self {
        Self {
            max_requests: 100,
            period_secs: 60, // 100 req/min
        }
    }
}

/// API key configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    /// Human-readable name
    pub name: String,
    /// The actual key (should be hashed in production)
    pub key: String,
    /// Granted permissions (parsed from strings like "gpu:read")
    #[serde(default)]
    pub permissions: Vec<Permission>,
    /// Legacy capability strings (for backward compatibility)
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// Rate limit override
    pub rate_limit: Option<RateLimit>,
    /// Whether this key is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Expiration time (optional)
    pub expires_at: Option<String>,
    /// Description/notes
    pub description: Option<String>,
}

fn default_true() -> bool {
    true
}

impl ApiKey {
    /// Create a new API key with specified permissions
    pub fn new(
        name: impl Into<String>,
        key: impl Into<String>,
        permissions: Vec<Permission>,
    ) -> Self {
        Self {
            name: name.into(),
            key: key.into(),
            permissions,
            capabilities: Vec::new(),
            rate_limit: None,
            enabled: true,
            expires_at: None,
            description: None,
        }
    }

    /// Create a read-only key with all monitoring capabilities
    pub fn read_only(name: impl Into<String>, key: impl Into<String>) -> Self {
        let permissions: Vec<Permission> = Capability::read_only_set()
            .into_iter()
            .map(Permission::read)
            .collect();
        Self::new(name, key, permissions)
    }

    /// Create an admin key with full access
    pub fn admin(name: impl Into<String>, key: impl Into<String>) -> Self {
        let permissions = vec![Permission::full(Capability::Admin)];
        Self::new(name, key, permissions)
    }

    /// Check if this key has a specific permission
    pub fn has_permission(&self, capability: Capability, required_scope: &Scope) -> bool {
        // Admin capability grants everything
        if self
            .permissions
            .iter()
            .any(|p| p.capability == Capability::Admin)
        {
            return true;
        }

        self.permissions.iter().any(|p| {
            p.capability == capability
                && match (&p.scope, required_scope) {
                    (Scope::All, _) => true,
                    (Scope::Write, Scope::Read) => true,
                    (Scope::Write, Scope::Write) => true,
                    (Scope::Read, Scope::Read) => true,
                    (Scope::Resource(granted), Scope::Resource(required)) => granted == required,
                    _ => false,
                }
        })
    }

    /// Get all granted capabilities
    pub fn granted_capabilities(&self) -> HashSet<Capability> {
        if self
            .permissions
            .iter()
            .any(|p| p.capability == Capability::Admin)
        {
            return Capability::all().into_iter().collect();
        }
        self.permissions.iter().map(|p| p.capability).collect()
    }
}

/// Rate limiter state
#[derive(Debug)]
pub struct RateLimiter {
    /// Request timestamps within the current window
    requests: Vec<Instant>,
    /// Rate limit configuration
    limit: RateLimit,
}

impl RateLimiter {
    /// Create a new rate limiter
    pub fn new(limit: RateLimit) -> Self {
        Self {
            requests: Vec::new(),
            limit,
        }
    }

    /// Check if a request is allowed
    pub fn check(&mut self) -> bool {
        let now = Instant::now();
        let window_start = now - self.limit.period();

        // Remove old requests outside the window
        self.requests.retain(|&t| t > window_start);

        if self.requests.len() < self.limit.max_requests as usize {
            self.requests.push(now);
            true
        } else {
            false
        }
    }

    /// Get remaining requests in current window
    pub fn remaining(&self) -> u32 {
        let now = Instant::now();
        let window_start = now - self.limit.period();
        let active_requests = self.requests.iter().filter(|&&t| t > window_start).count();
        self.limit
            .max_requests
            .saturating_sub(active_requests as u32)
    }

    /// Get time until next request is allowed
    pub fn retry_after(&self) -> Option<Duration> {
        if self.requests.len() < self.limit.max_requests as usize {
            return None;
        }

        let oldest = self.requests.first()?;
        let elapsed = oldest.elapsed();
        if elapsed < self.limit.period() {
            Some(self.limit.period() - elapsed)
        } else {
            None
        }
    }
}

/// Permission checker - validates requests against granted permissions
#[derive(Debug)]
pub struct PermissionChecker {
    /// API keys by key value
    keys: HashMap<String, ApiKey>,
    /// Rate limiters by key
    rate_limiters: HashMap<String, RateLimiter>,
    /// Default rate limit
    default_rate_limit: RateLimit,
}

impl PermissionChecker {
    /// Create a new permission checker with API keys
    pub fn new(keys: Vec<ApiKey>) -> Self {
        let key_map: HashMap<String, ApiKey> =
            keys.into_iter().map(|k| (k.key.clone(), k)).collect();
        Self {
            keys: key_map,
            rate_limiters: HashMap::new(),
            default_rate_limit: RateLimit::default(),
        }
    }

    /// Create a checker that allows everything (for local use)
    pub fn allow_all() -> Self {
        Self {
            keys: HashMap::new(),
            rate_limiters: HashMap::new(),
            default_rate_limit: RateLimit {
                max_requests: u32::MAX,
                period_secs: 1,
            },
        }
    }

    /// Get an API key by its value
    pub fn get_key(&self, key: &str) -> Option<&ApiKey> {
        self.keys.get(key)
    }

    /// Validate an API key and check permissions
    pub fn check(
        &mut self,
        api_key: &str,
        capability: Capability,
        scope: &Scope,
    ) -> Result<(), PermissionError> {
        // Get the key
        let key = self.keys.get(api_key).ok_or(PermissionError::InvalidKey)?;

        // Check if enabled
        if !key.enabled {
            return Err(PermissionError::KeyDisabled);
        }

        // Check rate limit
        let rate_limit = key
            .rate_limit
            .clone()
            .unwrap_or_else(|| self.default_rate_limit.clone());
        let limiter = self
            .rate_limiters
            .entry(api_key.to_string())
            .or_insert_with(|| RateLimiter::new(rate_limit));

        if !limiter.check() {
            return Err(PermissionError::RateLimited {
                retry_after: limiter.retry_after(),
            });
        }

        // Check permission
        if !key.has_permission(capability, scope) {
            return Err(PermissionError::AccessDenied {
                capability,
                scope: scope.clone(),
            });
        }

        Ok(())
    }

    /// Get granted capabilities for a key
    pub fn capabilities_for_key(&self, api_key: &str) -> HashSet<Capability> {
        self.keys
            .get(api_key)
            .map(|k| k.granted_capabilities())
            .unwrap_or_default()
    }

    /// Add or update an API key
    pub fn add_key(&mut self, key: ApiKey) {
        self.keys.insert(key.key.clone(), key);
    }

    /// Remove an API key
    pub fn remove_key(&mut self, api_key: &str) {
        self.keys.remove(api_key);
        self.rate_limiters.remove(api_key);
    }

    /// Get current rate limit status for an API key
    ///
    /// Returns (remaining_requests, total_limit, reset_timestamp) if the key exists.
    pub fn rate_limit_status(&self, api_key: &str) -> Option<(u32, u32, u64)> {
        let key = self.keys.get(api_key)?;
        let rate_limit = key
            .rate_limit
            .clone()
            .unwrap_or_else(|| self.default_rate_limit.clone());

        let remaining = self
            .rate_limiters
            .get(api_key)
            .map(|limiter| limiter.remaining())
            .unwrap_or(rate_limit.max_requests);

        // Reset time is current time + period
        let reset_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() + rate_limit.period_secs)
            .unwrap_or(0);

        Some((remaining, rate_limit.max_requests, reset_at))
    }
}

/// Permission check error
#[derive(Debug, Clone)]
pub enum PermissionError {
    /// Invalid or unknown API key
    InvalidKey,
    /// API key is disabled
    KeyDisabled,
    /// Rate limit exceeded
    RateLimited { retry_after: Option<Duration> },
    /// Access denied for capability
    AccessDenied {
        capability: Capability,
        scope: Scope,
    },
}

impl std::fmt::Display for PermissionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PermissionError::InvalidKey => write!(f, "Invalid API key"),
            PermissionError::KeyDisabled => write!(f, "API key is disabled"),
            PermissionError::RateLimited { retry_after } => {
                if let Some(d) = retry_after {
                    write!(f, "Rate limited. Retry after {:?}", d)
                } else {
                    write!(f, "Rate limited")
                }
            }
            PermissionError::AccessDenied { capability, scope } => {
                write!(f, "Access denied: {}:{:?}", capability, scope)
            }
        }
    }
}

impl std::error::Error for PermissionError {}

/// API configuration loaded from file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    /// Whether the API is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Bind address
    #[serde(default = "default_bind")]
    pub bind: String,
    /// Port number
    #[serde(default = "default_port")]
    pub port: u16,
    /// Require authentication
    #[serde(default = "default_true")]
    /// **Not consulted.** `ObservabilityApi::new` keeps only `keys`, and
    /// every API method looks its caller up in that map, so setting this to
    /// `false` does not disable authentication -- a keyless request is still
    /// refused with `InvalidKey`.
    ///
    /// Anonymous access is decided one layer out, at the request gate, by
    /// `ServerConfig::allow_anonymous`. That is the right place for it: the
    /// gate is what sees a request with no key at all. This field is kept
    /// because it is public API and because `http_server` sets it as
    /// documentation of intent, but it decides nothing.
    pub require_auth: bool,
    /// Allow unauthenticated read-only access
    #[serde(default)]
    /// **Not consulted.** See [`Self::require_auth`].
    pub allow_anonymous_read: bool,
    /// API keys
    #[serde(default)]
    pub keys: Vec<ApiKey>,
    /// Default rate limit for all keys
    #[serde(default)]
    pub default_rate_limit: Option<RateLimit>,
    /// Enable WebSocket streaming
    #[serde(default = "default_true")]
    pub enable_websocket: bool,
    /// Enable CORS
    #[serde(default)]
    pub enable_cors: bool,
    /// Allowed origins for CORS
    #[serde(default)]
    pub allowed_origins: Vec<String>,
}

fn default_bind() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    8787
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            bind: default_bind(),
            port: default_port(),
            require_auth: true,
            allow_anonymous_read: false,
            keys: Vec::new(),
            default_rate_limit: None,
            enable_websocket: true,
            enable_cors: false,
            allowed_origins: Vec::new(),
        }
    }
}

impl ApiConfig {
    /// Load configuration from TOML file
    pub fn load_from_file(path: &std::path::Path) -> Result<Self, Box<dyn std::error::Error>> {
        let contents = std::fs::read_to_string(path)?;
        let config: Self = toml::from_str(&contents)?;
        Ok(config)
    }

    /// Save configuration to TOML file
    pub fn save_to_file(&self, path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
        let contents = toml::to_string_pretty(self)?;
        std::fs::write(path, contents)?;
        Ok(())
    }

    /// Generate a new random API key
    pub fn generate_key() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        format!("sk-ironmon-{:x}", timestamp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_parse() {
        let p = Permission::parse("gpu:read").unwrap();
        assert_eq!(p.capability, Capability::Gpu);
        assert_eq!(p.scope, Scope::Read);

        let p = Permission::parse("process").unwrap();
        assert_eq!(p.capability, Capability::Process);
        assert_eq!(p.scope, Scope::Read);

        let p = Permission::parse("admin:*").unwrap();
        assert_eq!(p.capability, Capability::Admin);
        assert_eq!(p.scope, Scope::All);
    }

    #[test]
    fn test_api_key_permissions() {
        let key = ApiKey::read_only("test", "sk-test");
        assert!(key.has_permission(Capability::Gpu, &Scope::Read));
        assert!(!key.has_permission(Capability::GpuControl, &Scope::Write));

        let admin = ApiKey::admin("admin", "sk-admin");
        assert!(admin.has_permission(Capability::GpuControl, &Scope::Write));
        assert!(admin.has_permission(Capability::Admin, &Scope::All));
    }

    #[test]
    fn test_rate_limiter() {
        let mut limiter = RateLimiter::new(RateLimit {
            max_requests: 3,
            period_secs: 60,
        });

        assert!(limiter.check());
        assert!(limiter.check());
        assert!(limiter.check());
        assert!(!limiter.check()); // Should be rate limited
        assert_eq!(limiter.remaining(), 0);
    }

    #[test]
    fn test_rate_limit_status_unknown_key() {
        let checker = PermissionChecker::new(vec![]);
        assert!(checker.rate_limit_status("nonexistent").is_none());
    }

    #[test]
    fn test_rate_limit_status_known_key() {
        let key = ApiKey::read_only("test", "sk-test");
        let checker = PermissionChecker::new(vec![key]);
        let status = checker.rate_limit_status("sk-test");
        assert!(status.is_some());
        let (remaining, limit, reset_at) = status.unwrap();
        // Default rate limit: 100 requests per 60 seconds
        assert_eq!(limit, 100);
        assert_eq!(remaining, 100); // No requests made yet
        assert!(reset_at > 0);
    }

    #[test]
    fn test_rate_limit_status_after_requests() {
        let key = ApiKey::new("test", "sk-test", vec![Permission::read(Capability::Gpu)]);
        let mut checker = PermissionChecker::new(vec![key]);

        // Make some requests
        let _ = checker.check("sk-test", Capability::Gpu, &Scope::Read);
        let _ = checker.check("sk-test", Capability::Gpu, &Scope::Read);

        let (remaining, limit, _) = checker.rate_limit_status("sk-test").unwrap();
        assert_eq!(limit, 100);
        assert_eq!(remaining, 98); // 2 requests consumed
    }

    #[test]
    fn test_rate_limit_status_with_custom_limit() {
        let mut key = ApiKey::read_only("test", "sk-test");
        key.rate_limit = Some(RateLimit {
            max_requests: 10,
            period_secs: 30,
        });
        let checker = PermissionChecker::new(vec![key]);
        let (remaining, limit, _) = checker.rate_limit_status("sk-test").unwrap();
        assert_eq!(limit, 10);
        assert_eq!(remaining, 10);
    }
}
