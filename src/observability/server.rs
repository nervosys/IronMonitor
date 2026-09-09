//! HTTP/WebSocket Server for External API Access
//!
//! This module provides a REST API server for external AI systems to query
//! the observability API. It includes:
//! - REST endpoints for all observability queries
//! - WebSocket endpoint for real-time streaming
//! - Authentication middleware
//! - Rate limiting

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::api::*;
use super::events::*;
use super::metrics::*;

/// Counter for unique request IDs
static REQUEST_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generate a unique request ID using timestamp + counter
fn generate_request_id() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let counter = REQUEST_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("req-{}-{}", timestamp, counter)
}

/// Server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Bind address
    pub bind_address: String,
    /// Port
    pub port: u16,
    /// Enable HTTPS
    pub tls_enabled: bool,
    /// TLS certificate path
    pub tls_cert_path: Option<String>,
    /// TLS key path
    pub tls_key_path: Option<String>,
    /// Enable CORS
    pub cors_enabled: bool,
    /// Allowed origins for CORS
    pub cors_origins: Vec<String>,
    /// Enable request logging
    pub request_logging: bool,
    /// Max request body size
    pub max_body_size: usize,
    /// Request timeout (seconds)
    pub request_timeout_secs: u64,
    /// Serve read-only endpoints without an API key.
    ///
    /// The request gate rejects any keyless request outright, and it cannot see
    /// [`crate::observability::permissions::ApiConfig`] — `ObservabilityApi::new`
    /// keeps only `config.keys` and drops `require_auth` / `allow_anonymous_read`.
    /// So the flag has to live here to reach the gate at all.
    ///
    /// Intended for a loopback bind, where a local user could read the same
    /// telemetry by running ironmon directly. Leave it false when binding to a
    /// routable address.
    pub allow_anonymous: bool,
    /// Enable WebSocket
    pub websocket_enabled: bool,
    /// Max WebSocket connections
    pub max_websocket_connections: usize,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1".to_string(),
            port: 8080,
            tls_enabled: false,
            tls_cert_path: None,
            tls_key_path: None,
            cors_enabled: true,
            cors_origins: vec!["*".to_string()],
            request_logging: true,
            max_body_size: 1024 * 1024, // 1MB
            request_timeout_secs: 30,
            // Default to requiring a key: a caller constructing ServerConfig directly
            // has not told us the bind is loopback, and the safe assumption is that
            // it is not.
            allow_anonymous: false,
            websocket_enabled: true,
            max_websocket_connections: 100,
        }
    }
}

/// API route definitions
pub mod routes {
    /// API version prefix
    pub const API_V1: &str = "/api/v1";

    /// Health check
    pub const HEALTH: &str = "/health";

    /// System context
    pub const CONTEXT: &str = "/context";
    pub const CONTEXT_MINIMAL: &str = "/context/minimal";

    /// Hardware inventory
    pub const GPUS: &str = "/gpus";
    pub const CPU: &str = "/cpu";
    pub const MEMORY: &str = "/memory";
    pub const DISKS: &str = "/disks";
    pub const NETWORK: &str = "/network";
    pub const MOTHERBOARD: &str = "/motherboard";
    pub const POWER: &str = "/power";
    pub const FANS: &str = "/fans";
    pub const TEMPERATURES: &str = "/temperatures";

    /// Metrics
    pub const METRICS: &str = "/metrics";
    pub const METRICS_GPU: &str = "/metrics/gpu";
    pub const METRICS_CPU: &str = "/metrics/cpu";
    pub const METRICS_MEMORY: &str = "/metrics/memory";
    pub const METRICS_DISK: &str = "/metrics/disk";
    pub const METRICS_NETWORK: &str = "/metrics/network";
    pub const METRICS_PROMETHEUS: &str = "/metrics/prometheus";

    /// Processes
    pub const PROCESSES: &str = "/processes";
    pub const PROCESS_BY_ID: &str = "/processes/:pid";

    /// Events
    pub const EVENTS: &str = "/events";
    pub const EVENTS_SUBSCRIBE: &str = "/events/subscribe";

    /// Streaming
    pub const STREAM: &str = "/stream";

    /// API info
    pub const CAPABILITIES: &str = "/capabilities";
    pub const OPENAPI: &str = "/openapi.json";
}

/// HTTP request
#[derive(Debug)]
pub struct HttpRequest {
    /// HTTP method
    pub method: String,
    /// Request path
    pub path: String,
    /// Query parameters
    pub query: HashMap<String, String>,
    /// Headers
    pub headers: HashMap<String, String>,
    /// Body
    pub body: Option<String>,
    /// Client address
    pub client_addr: Option<SocketAddr>,
}

impl HttpRequest {
    /// Get a header value
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(&name.to_lowercase()).map(|s| s.as_str())
    }

    /// Get the API key from Authorization header
    pub fn api_key(&self) -> Option<&str> {
        self.header("authorization")
            .and_then(|h| h.strip_prefix("Bearer "))
    }

    /// Get a query parameter
    pub fn query_param(&self, name: &str) -> Option<&str> {
        self.query.get(name).map(|s| s.as_str())
    }
}

/// HTTP response
#[derive(Debug)]
pub struct HttpResponse {
    /// Status code
    pub status: u16,
    /// Headers
    pub headers: HashMap<String, String>,
    /// Body
    pub body: String,
}

impl HttpResponse {
    /// Create a JSON response
    pub fn json<T: Serialize>(status: u16, data: &T) -> Self {
        let body = serde_json::to_string(data).unwrap_or_default();
        let mut headers = HashMap::new();
        headers.insert("content-type".to_string(), "application/json".to_string());

        Self {
            status,
            headers,
            body,
        }
    }

    /// Create an error response
    pub fn error(status: u16, message: &str) -> Self {
        Self::json(
            status,
            &serde_json::json!({
                "error": message,
                "status": status
            }),
        )
    }

    /// 200 OK
    pub fn ok<T: Serialize>(data: &T) -> Self {
        Self::json(200, data)
    }

    /// 201 Created
    pub fn created<T: Serialize>(data: &T) -> Self {
        Self::json(201, data)
    }

    /// 204 No Content
    pub fn no_content() -> Self {
        Self {
            status: 204,
            headers: HashMap::new(),
            body: String::new(),
        }
    }

    /// 400 Bad Request
    pub fn bad_request(message: &str) -> Self {
        Self::error(400, message)
    }

    /// 401 Unauthorized
    pub fn unauthorized(message: &str) -> Self {
        Self::error(401, message)
    }

    /// 403 Forbidden
    pub fn forbidden(message: &str) -> Self {
        Self::error(403, message)
    }

    /// 404 Not Found
    pub fn not_found(message: &str) -> Self {
        Self::error(404, message)
    }

    /// 429 Too Many Requests
    pub fn rate_limited(retry_after: u64) -> Self {
        let mut response = Self::error(429, "Rate limit exceeded");
        response
            .headers
            .insert("retry-after".to_string(), retry_after.to_string());
        response
    }

    /// 500 Internal Server Error
    pub fn internal_error(message: &str) -> Self {
        Self::error(500, message)
    }
}

/// OpenAPI specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiSpec {
    pub openapi: String,
    pub info: OpenApiInfo,
    pub servers: Vec<OpenApiServer>,
    pub paths: HashMap<String, OpenApiPath>,
    pub components: OpenApiComponents,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiInfo {
    pub title: String,
    pub description: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiServer {
    pub url: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiPath {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub get: Option<OpenApiOperation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post: Option<OpenApiOperation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub put: Option<OpenApiOperation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delete: Option<OpenApiOperation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiOperation {
    pub summary: String,
    pub description: String,
    #[serde(rename = "operationId")]
    pub operation_id: String,
    pub tags: Vec<String>,
    pub security: Vec<HashMap<String, Vec<String>>>,
    pub responses: HashMap<String, OpenApiResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiResponse {
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiComponents {
    #[serde(rename = "securitySchemes")]
    pub security_schemes: HashMap<String, OpenApiSecurityScheme>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenApiSecurityScheme {
    #[serde(rename = "type")]
    pub scheme_type: String,
    pub scheme: String,
    #[serde(rename = "bearerFormat")]
    pub bearer_format: String,
}

impl OpenApiSpec {
    /// Generate the OpenAPI spec for the observability API
    pub fn generate(base_url: &str) -> Self {
        let mut paths = HashMap::new();

        // Health check
        paths.insert(
            routes::HEALTH.to_string(),
            OpenApiPath {
                get: Some(OpenApiOperation {
                    summary: "Health check".to_string(),
                    description: "Check if the API is healthy".to_string(),
                    operation_id: "health".to_string(),
                    tags: vec!["system".to_string()],
                    security: vec![],
                    responses: HashMap::from([(
                        "200".to_string(),
                        OpenApiResponse {
                            description: "OK".to_string(),
                        },
                    )]),
                }),
                post: None,
                put: None,
                delete: None,
            },
        );

        // Context
        paths.insert(
            format!("{}{}", routes::API_V1, routes::CONTEXT),
            OpenApiPath {
                get: Some(OpenApiOperation {
                    summary: "Get full system context".to_string(),
                    description: "Returns a complete snapshot of the system state".to_string(),
                    operation_id: "getContext".to_string(),
                    tags: vec!["context".to_string()],
                    security: vec![HashMap::from([("bearerAuth".to_string(), vec![])])],
                    responses: HashMap::from([
                        (
                            "200".to_string(),
                            OpenApiResponse {
                                description: "System context".to_string(),
                            },
                        ),
                        (
                            "401".to_string(),
                            OpenApiResponse {
                                description: "Unauthorized".to_string(),
                            },
                        ),
                    ]),
                }),
                post: None,
                put: None,
                delete: None,
            },
        );

        // GPUs
        paths.insert(
            format!("{}{}", routes::API_V1, routes::GPUS),
            OpenApiPath {
                get: Some(OpenApiOperation {
                    summary: "Get GPU information".to_string(),
                    description: "Returns information about all GPUs".to_string(),
                    operation_id: "getGpus".to_string(),
                    tags: vec!["hardware".to_string()],
                    security: vec![HashMap::from([("bearerAuth".to_string(), vec![])])],
                    responses: HashMap::from([(
                        "200".to_string(),
                        OpenApiResponse {
                            description: "GPU list".to_string(),
                        },
                    )]),
                }),
                post: None,
                put: None,
                delete: None,
            },
        );

        // Add more paths...

        let mut security_schemes = HashMap::new();
        security_schemes.insert(
            "bearerAuth".to_string(),
            OpenApiSecurityScheme {
                scheme_type: "http".to_string(),
                scheme: "bearer".to_string(),
                bearer_format: "API Key".to_string(),
            },
        );

        Self {
            openapi: "3.0.3".to_string(),
            info: OpenApiInfo {
                title: "IronMonitor Observability API".to_string(),
                description: "Full system observability API with MCP-like permissions".to_string(),
                version: "1.0.0".to_string(),
            },
            servers: vec![OpenApiServer {
                url: base_url.to_string(),
                description: "Local server".to_string(),
            }],
            paths,
            components: OpenApiComponents { security_schemes },
        }
    }
}

/// Request handler context
pub struct RequestHandler {
    pub api: Arc<std::sync::RwLock<ObservabilityApi>>,
    pub event_manager: Arc<EventManager>,
    pub metric_collector: Arc<MetricCollector>,
    pub config: ServerConfig,
}

impl RequestHandler {
    pub fn new(
        api: ObservabilityApi,
        event_manager: EventManager,
        metric_collector: MetricCollector,
        config: ServerConfig,
    ) -> Self {
        Self {
            api: Arc::new(std::sync::RwLock::new(api)),
            event_manager: Arc::new(event_manager),
            metric_collector: Arc::new(metric_collector),
            config,
        }
    }

    /// Handle an HTTP request
    pub fn handle(&self, request: HttpRequest) -> HttpResponse {
        let start = Instant::now();

        // Extract API key.
        //
        // Keyless requests were rejected unconditionally, with only /health exempt,
        // which made the entire REST API and the Prometheus endpoint unreachable
        // unless the caller happened to send a Bearer token — and no configuration
        // path registered one. `allow_anonymous` restores a usable loopback default
        // without weakening a server that was deliberately given keys.
        let api_key = match request.api_key() {
            Some(key) => key.to_string(),
            None if request.path == routes::HEALTH => String::new(),
            None if self.config.allow_anonymous => String::new(),
            None => return HttpResponse::unauthorized("Missing API key"),
        };

        // Create request context
        let ctx = RequestContext::new(api_key.clone()).with_request_id(generate_request_id());

        // Route the request
        let response = self.route(&request, ctx);

        // Log request
        if self.config.request_logging {
            let duration = start.elapsed();
            log::info!(
                "{} {} {} {}ms",
                request.method,
                request.path,
                response.status,
                duration.as_millis()
            );
        }

        response
    }

    fn route(&self, request: &HttpRequest, ctx: RequestContext) -> HttpResponse {
        let path = request.path.as_str();
        let method = request.method.as_str();

        // Health check (no auth required)
        if path == routes::HEALTH && method == "GET" {
            return HttpResponse::ok(&serde_json::json!({
                "status": "healthy",
                "timestamp": std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0)
            }));
        }

        // OpenAPI spec
        let openapi_path = format!("{}{}", routes::API_V1, routes::OPENAPI);
        if path == openapi_path && method == "GET" {
            let base_url = format!(
                "http{}://{}:{}",
                if self.config.tls_enabled { "s" } else { "" },
                self.config.bind_address,
                self.config.port
            );
            return HttpResponse::ok(&OpenApiSpec::generate(&base_url));
        }

        // API v1 routes
        let api_prefix = routes::API_V1;
        if !path.starts_with(api_prefix) {
            return HttpResponse::not_found("Unknown route");
        }

        let sub_path = &path[api_prefix.len()..];

        match (method, sub_path) {
            // Context
            ("GET", path) if path == routes::CONTEXT => self.handle_get_context(ctx),
            ("GET", path) if path == routes::CONTEXT_MINIMAL => {
                self.handle_get_minimal_context(ctx)
            }

            // Hardware
            ("GET", path) if path == routes::GPUS => self.handle_get_gpus(ctx),
            ("GET", path) if path == routes::CPU => self.handle_get_cpu(ctx),
            ("GET", path) if path == routes::MEMORY => self.handle_get_memory(ctx),
            ("GET", path) if path == routes::DISKS => self.handle_get_disks(ctx),
            ("GET", path) if path == routes::NETWORK => self.handle_get_network(ctx),
            ("GET", path) if path == routes::MOTHERBOARD => self.handle_get_motherboard(ctx),
            ("GET", path) if path == routes::POWER => self.handle_get_power(ctx),
            ("GET", path) if path == routes::FANS => self.handle_get_fans(ctx),
            ("GET", path) if path == routes::TEMPERATURES => self.handle_get_temperatures(ctx),

            // Metrics
            ("GET", path) if path == routes::METRICS_GPU => self.handle_get_gpu_metrics(ctx),
            ("GET", path) if path == routes::METRICS_CPU => self.handle_get_cpu_metrics(ctx),
            ("GET", path) if path == routes::METRICS_MEMORY => self.handle_get_memory_metrics(ctx),
            ("GET", path) if path == routes::METRICS_DISK => self.handle_get_disk_metrics(ctx),
            ("GET", path) if path == routes::METRICS_NETWORK => {
                self.handle_get_network_metrics(ctx)
            }
            ("GET", path) if path == routes::METRICS_PROMETHEUS => self.handle_prometheus_metrics(),

            // Processes
            ("GET", path) if path == routes::PROCESSES => self.handle_get_processes(ctx, request),

            // Events
            ("GET", path) if path == routes::EVENTS => self.handle_get_events(request),

            // Capabilities
            ("GET", path) if path == routes::CAPABILITIES => self.handle_get_capabilities(ctx),

            _ => HttpResponse::not_found("Unknown route"),
        }
    }

    fn handle_get_context(&self, ctx: RequestContext) -> HttpResponse {
        match self.api.write() {
            Ok(mut api) => match api.get_context(&ctx) {
                Ok(response) => HttpResponse::ok(&response),
                Err(e) => self.error_response(e),
            },
            Err(_) => HttpResponse::internal_error("Failed to acquire API lock"),
        }
    }

    fn handle_get_minimal_context(&self, ctx: RequestContext) -> HttpResponse {
        match self.api.write() {
            Ok(mut api) => match api.get_minimal_context(&ctx) {
                Ok(response) => HttpResponse::ok(&response),
                Err(e) => self.error_response(e),
            },
            Err(_) => HttpResponse::internal_error("Failed to acquire API lock"),
        }
    }

    fn handle_get_gpus(&self, ctx: RequestContext) -> HttpResponse {
        match self.api.read() {
            Ok(api) => match api.get_gpus(&ctx) {
                Ok(response) => HttpResponse::ok(&response),
                Err(e) => self.error_response(e),
            },
            Err(_) => HttpResponse::internal_error("Failed to acquire API lock"),
        }
    }

    fn handle_get_cpu(&self, ctx: RequestContext) -> HttpResponse {
        match self.api.read() {
            Ok(api) => match api.get_cpu_metrics(&ctx) {
                Ok(response) => HttpResponse::ok(&response),
                Err(e) => self.error_response(e),
            },
            Err(_) => HttpResponse::internal_error("Failed to acquire API lock"),
        }
    }

    fn handle_get_memory(&self, ctx: RequestContext) -> HttpResponse {
        match self.api.read() {
            Ok(api) => match api.get_memory_metrics(&ctx) {
                Ok(response) => HttpResponse::ok(&response),
                Err(e) => self.error_response(e),
            },
            Err(_) => HttpResponse::internal_error("Failed to acquire API lock"),
        }
    }

    fn handle_get_disks(&self, ctx: RequestContext) -> HttpResponse {
        match self.api.read() {
            Ok(api) => match api.get_disks(&ctx) {
                Ok(response) => HttpResponse::ok(&response),
                Err(e) => self.error_response(e),
            },
            Err(_) => HttpResponse::internal_error("Failed to acquire API lock"),
        }
    }

    fn handle_get_network(&self, ctx: RequestContext) -> HttpResponse {
        match self.api.read() {
            Ok(api) => match api.get_network_interfaces(&ctx) {
                Ok(response) => HttpResponse::ok(&response),
                Err(e) => self.error_response(e),
            },
            Err(_) => HttpResponse::internal_error("Failed to acquire API lock"),
        }
    }

    fn handle_get_motherboard(&self, ctx: RequestContext) -> HttpResponse {
        match self.api.read() {
            Ok(api) => match api.get_motherboard(&ctx) {
                Ok(response) => HttpResponse::ok(&response),
                Err(e) => self.error_response(e),
            },
            Err(_) => HttpResponse::internal_error("Failed to acquire API lock"),
        }
    }

    fn handle_get_power(&self, ctx: RequestContext) -> HttpResponse {
        match self.api.read() {
            Ok(api) => match api.get_power_status(&ctx) {
                Ok(response) => HttpResponse::ok(&response),
                Err(e) => self.error_response(e),
            },
            Err(_) => HttpResponse::internal_error("Failed to acquire API lock"),
        }
    }

    fn handle_get_fans(&self, ctx: RequestContext) -> HttpResponse {
        match self.api.read() {
            Ok(api) => match api.get_fans(&ctx) {
                Ok(response) => HttpResponse::ok(&response),
                Err(e) => self.error_response(e),
            },
            Err(_) => HttpResponse::internal_error("Failed to acquire API lock"),
        }
    }

    fn handle_get_temperatures(&self, ctx: RequestContext) -> HttpResponse {
        match self.api.read() {
            Ok(api) => match api.get_temperatures(&ctx) {
                Ok(response) => HttpResponse::ok(&response),
                Err(e) => self.error_response(e),
            },
            Err(_) => HttpResponse::internal_error("Failed to acquire API lock"),
        }
    }

    fn handle_get_gpu_metrics(&self, ctx: RequestContext) -> HttpResponse {
        match self.api.read() {
            Ok(api) => match api.get_gpu_metrics(&ctx) {
                Ok(response) => HttpResponse::ok(&response),
                Err(e) => self.error_response(e),
            },
            Err(_) => HttpResponse::internal_error("Failed to acquire API lock"),
        }
    }

    fn handle_get_cpu_metrics(&self, ctx: RequestContext) -> HttpResponse {
        match self.api.read() {
            Ok(api) => match api.get_cpu_metrics(&ctx) {
                Ok(response) => HttpResponse::ok(&response),
                Err(e) => self.error_response(e),
            },
            Err(_) => HttpResponse::internal_error("Failed to acquire API lock"),
        }
    }

    fn handle_get_memory_metrics(&self, ctx: RequestContext) -> HttpResponse {
        match self.api.read() {
            Ok(api) => match api.get_memory_metrics(&ctx) {
                Ok(response) => HttpResponse::ok(&response),
                Err(e) => self.error_response(e),
            },
            Err(_) => HttpResponse::internal_error("Failed to acquire API lock"),
        }
    }

    fn handle_get_disk_metrics(&self, ctx: RequestContext) -> HttpResponse {
        match self.api.read() {
            Ok(api) => match api.get_disk_metrics(&ctx) {
                Ok(response) => HttpResponse::ok(&response),
                Err(e) => self.error_response(e),
            },
            Err(_) => HttpResponse::internal_error("Failed to acquire API lock"),
        }
    }

    fn handle_get_network_metrics(&self, ctx: RequestContext) -> HttpResponse {
        match self.api.read() {
            Ok(api) => match api.get_network_metrics(&ctx) {
                Ok(response) => HttpResponse::ok(&response),
                Err(e) => self.error_response(e),
            },
            Err(_) => HttpResponse::internal_error("Failed to acquire API lock"),
        }
    }

    fn handle_prometheus_metrics(&self) -> HttpResponse {
        let metrics = self.metric_collector.export_prometheus();
        HttpResponse {
            status: 200,
            headers: HashMap::from([(
                "content-type".to_string(),
                "text/plain; charset=utf-8".to_string(),
            )]),
            body: metrics,
        }
    }

    fn handle_get_processes(&self, ctx: RequestContext, request: &HttpRequest) -> HttpResponse {
        let limit = request.query_param("limit").and_then(|s| s.parse().ok());

        match self.api.read() {
            Ok(api) => match api.get_processes(&ctx, limit) {
                Ok(response) => HttpResponse::ok(&response),
                Err(e) => self.error_response(e),
            },
            Err(_) => HttpResponse::internal_error("Failed to acquire API lock"),
        }
    }

    fn handle_get_events(&self, request: &HttpRequest) -> HttpResponse {
        let limit = request.query_param("limit").and_then(|s| s.parse().ok());
        let since = request.query_param("since").and_then(|s| s.parse().ok());

        let events = if let Some(since) = since {
            self.event_manager.get_events_since(since)
        } else {
            self.event_manager.get_events(None, limit)
        };

        HttpResponse::ok(&events)
    }

    fn handle_get_capabilities(&self, ctx: RequestContext) -> HttpResponse {
        match self.api.read() {
            Ok(api) => match api.list_capabilities(&ctx) {
                Ok(response) => HttpResponse::ok(&response),
                Err(e) => self.error_response(e),
            },
            Err(_) => HttpResponse::internal_error("Failed to acquire API lock"),
        }
    }

    fn error_response(&self, error: ObservabilityError) -> HttpResponse {
        match error {
            ObservabilityError::PermissionDenied(msg) => HttpResponse::forbidden(&msg),
            ObservabilityError::RateLimited { retry_after_secs } => {
                HttpResponse::rate_limited(retry_after_secs)
            }
            ObservabilityError::NotFound(msg) => HttpResponse::not_found(&msg),
            ObservabilityError::InvalidRequest(msg) => HttpResponse::bad_request(&msg),
            ObservabilityError::NotAvailable(msg) => HttpResponse::not_found(&msg),
            ObservabilityError::Internal(msg) => HttpResponse::internal_error(&msg),
        }
    }
}
