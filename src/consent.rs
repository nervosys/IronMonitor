//! User Consent Management
//!
//! # IronMonitor sends system telemetry to nervosys, on by default
//!
//! Read this before anything else in the module.
//!
//! `ironmon serve` sends the readings it takes to **telemetry.nervosys.ai**
//! every 15 minutes, as OTLP/HTTP, with a random install id. That is the
//! [`ConsentScope::SystemTelemetry`] scope, and it is the **only** scope that
//! collects anything. It is **on unless the user opts out**:
//! `ironmon privacy opt-out`, `--no-telemetry`, `--offline`,
//! `IRONMON_NO_TELEMETRY=1` or `DO_NOT_TRACK=1`. Consent is read again before
//! every send, so opting out stops the next one without a restart. Nothing is
//! sent from a detected sandbox.
//!
//! What is sent is everything `ironmon serve` reads: hardware models and
//! limits, utilisation, clocks, temperatures, power, memory, disks and their
//! mount points, network interface names and byte counters, the host name, the
//! largest processes by name, inference engine counters and agent session
//! counts. Every value carries its provenance. Endpoint credentials and API
//! keys are redacted from everything served and are not sent.
//!
//! The other five scopes describe collectors that were never built.
//! [`ConsentScope::is_collected`] is `false` for them, and their preference is
//! recorded but switches nothing on.
//!
//! **Who chose the destination** still separates telemetry from storage. The
//! local OTLP push (`localhost:4318` by default), [`crate::tsdb`],
//! [`crate::prometheus`] and the optional `fleet-store` write where the
//! *operator* points them, and are the operator's data. telemetry.nervosys.ai
//! is chosen by us, which is why it is a consent scope, disclosed here, on the
//! `ironmon privacy` screens and in the `ironmon serve` banner.
//!
//! # Principles
//!
//! - **Defaults are stated, not implied**: [`ConsentScope::default_granted`]
//!   is `true` for System Telemetry (opt-out) and `false` for every other
//!   scope (denied until asked). An unanswered scope takes that default, and
//!   `ironmon privacy status` says which it is.
//! - **Easy opt-out**: `ironmon privacy opt-out` records a denial for every scope,
//!   and the session flags and `DO_NOT_TRACK` override any recorded answer
//! - **Transparency**: the disclosure says what is collected and what is not
//! - **Auditability**: every consent decision is stored with the time it was made
//!
//! No claim is made here about compliance with any particular privacy
//! regulation. That is a question about a deployment and its operator, not
//! about a library, and this module is not in a position to answer it.
//!
//! # Example
//!
//! ```no_run
//! use ironmonitor::consent::{ConsentManager, ConsentScope};
//!
//! let mut manager = ConsentManager::load()?;
//!
//! // Check if user has consented to basic telemetry
//! if !manager.has_consent(ConsentScope::BasicTelemetry) {
//!     // Prompt user for consent
//!     manager.request_consent(ConsentScope::BasicTelemetry)?;
//! }
//!
//! // Only collect data if user explicitly consented
//! if manager.has_consent(ConsentScope::BasicTelemetry) {
//!     // Safe to collect anonymized usage data
//! }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use crate::error::{IronError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Check if telemetry is disabled for this session via CLI flags.
///
/// Returns `true` if the `--no-telemetry` or `--offline` CLI flag was used.
/// This check is fast and can be used early in code paths to skip telemetry logic entirely.
///
/// # Example
///
/// ```no_run
/// use ironmonitor::consent::is_telemetry_disabled;
///
/// if is_telemetry_disabled() {
///     // Skip any telemetry code paths
///     return;
/// }
/// ```
pub fn is_telemetry_disabled() -> bool {
    std::env::var("IRONMON_NO_TELEMETRY").is_ok() || std::env::var("IRONMON_OFFLINE").is_ok()
}

/// Check if running in offline mode (all network features disabled).
///
/// Returns `true` if the `--offline` CLI flag was used.
///
/// # Example
///
/// ```no_run
/// use ironmonitor::consent::is_offline_mode;
///
/// if is_offline_mode() {
///     // Skip any network-dependent features
///     return;
/// }
/// ```
pub fn is_offline_mode() -> bool {
    std::env::var("IRONMON_OFFLINE").is_ok()
}

/// Why collection is refused for this process regardless of recorded consent,
/// if it is: the session flags (`--no-telemetry`, `--offline`) or the
/// `DO_NOT_TRACK` convention. `DO_NOT_TRACK` counts when set to anything but
/// empty, `0` or `false`.
pub fn runtime_opt_out() -> Option<&'static str> {
    if std::env::var("IRONMON_OFFLINE").is_ok() {
        return Some("--offline");
    }
    if std::env::var("IRONMON_NO_TELEMETRY").is_ok() {
        return Some("--no-telemetry");
    }
    let dnt = std::env::var("DO_NOT_TRACK").unwrap_or_default();
    let dnt = dnt.trim();
    if !dnt.is_empty() && dnt != "0" && !dnt.eq_ignore_ascii_case("false") {
        return Some("DO_NOT_TRACK");
    }
    None
}

/// Consent scopes - each represents a specific type of data collection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsentScope {
    /// Every reading `ironmon serve` takes, sent to telemetry.nervosys.ai.
    /// On by default; the one scope that collects.
    SystemTelemetry,

    /// A full device fingerprint and profile, including hardware serials and
    /// the user's account name, full name and home directory, sent once per
    /// `ironmon serve` session. Off until the user agrees.
    DeviceProfile,

    /// Basic anonymized usage telemetry (feature usage, crash reports)
    BasicTelemetry,

    /// Hardware information (GPU model, CPU type - anonymized)
    HardwareInfo,

    /// Performance metrics (aggregated, anonymized)
    PerformanceMetrics,

    /// Detailed diagnostics (for troubleshooting, includes system info)
    DetailedDiagnostics,

    /// Anonymous analytics (usage patterns, feature popularity)
    Analytics,
}

impl ConsentScope {
    /// Every scope, in the order the privacy screens list them.
    pub const ALL: [ConsentScope; 7] = [
        ConsentScope::SystemTelemetry,
        ConsentScope::DeviceProfile,
        ConsentScope::BasicTelemetry,
        ConsentScope::HardwareInfo,
        ConsentScope::PerformanceMetrics,
        ConsentScope::DetailedDiagnostics,
        ConsentScope::Analytics,
    ];

    /// What an unanswered scope means. System Telemetry is opt-out; every other
    /// scope is denied until the user agrees.
    pub fn default_granted(&self) -> bool {
        matches!(self, Self::SystemTelemetry)
    }

    /// Get human-readable name
    pub fn name(&self) -> &'static str {
        match self {
            Self::SystemTelemetry => "System Telemetry",
            Self::DeviceProfile => "Device Profile",
            Self::BasicTelemetry => "Basic Telemetry",
            Self::HardwareInfo => "Hardware Information",
            Self::PerformanceMetrics => "Performance Metrics",
            Self::DetailedDiagnostics => "Detailed Diagnostics",
            Self::Analytics => "Anonymous Analytics",
        }
    }

    /// Get detailed description of what's collected
    pub fn description(&self) -> &'static str {
        match self {
            Self::SystemTelemetry =>
                "Everything `ironmon serve` reads, sent to telemetry.nervosys.ai every 15 minutes \
                 with a random install id. ON BY DEFAULT: `ironmon privacy opt-out`, \
                 --no-telemetry or DO_NOT_TRACK=1 stops it. This is not anonymous: it includes \
                 the host name, process names, network interface names and disk mount points.",
            Self::DeviceProfile =>
                "A full fingerprint and profile of this device, sent to telemetry.nervosys.ai once \
                 per `ironmon serve` session alongside System Telemetry, and kept by nervosys. \
                 OFF UNTIL YOU AGREE. It identifies the machine and its user: hardware UUIDs and \
                 serial numbers, the Windows machine GUID or Linux machine-id, MAC addresses, \
                 your account name, full name, home directory, domain and whether you are an \
                 administrator, plus the hardware, displays, OS, and Ollama models you have pulled. \
                 Values are sent unhashed.",
            Self::BasicTelemetry =>
                "Basic usage statistics and crash reports. Helps improve stability and reliability. \
                 No personally identifiable information is collected.",
            Self::HardwareInfo =>
                "Anonymized hardware information (GPU models, CPU types, RAM size). \
                 Helps prioritize hardware support. Serial numbers and MAC addresses are never collected.",
            Self::PerformanceMetrics =>
                "Aggregated performance metrics (frame rates, GPU utilization averages). \
                 Helps optimize performance. All data is anonymized and aggregated.",
            Self::DetailedDiagnostics =>
                "Detailed system information for troubleshooting (driver versions, kernel info). \
                 Only collected when explicitly requested for bug reports.",
            Self::Analytics =>
                "Anonymous analytics about feature usage patterns. \
                 Helps understand which features are most valuable. No tracking or profiling.",
        }
    }

    /// Whether any code in this crate actually collects or transmits data
    /// under this scope.
    ///
    /// Every arm is `false`, and that is the point. The five scopes, their
    /// descriptions and their itemised data points were written for a
    /// collector that was never built, and `ironmon privacy` displayed them as
    /// though the collection were happening and merely switched off.
    ///
    /// **If you add a collector, flip its scope here in the same change.**
    /// [`ConsentManager::should_collect_data`] consults this, so a scope that
    /// nothing implements cannot report that it should collect, however the
    /// user answered. `granting_an_unimplemented_scope_still_collects_nothing` in this
    /// module fails the moment an arm becomes `true`, which is the moment the
    /// disclosure and the CLI text need revisiting.
    pub fn is_collected(&self) -> bool {
        match self {
            // `ironmon serve` sends this; see `crate::telemetry`.
            Self::SystemTelemetry => true,
            // `ironmon serve` sends this once per session; see `crate::device_profile`.
            Self::DeviceProfile => true,
            Self::BasicTelemetry
            | Self::HardwareInfo
            | Self::PerformanceMetrics
            | Self::DetailedDiagnostics
            | Self::Analytics => false,
        }
    }

    /// The data points that *would* be collected under this scope if it were
    /// implemented. Nothing here is gathered today — see [`Self::is_collected`].
    pub fn data_points(&self) -> Vec<&'static str> {
        match self {
            Self::SystemTelemetry => vec![
                "A random install id and the IronMonitor version",
                "Host name, operating system, kernel and boot details",
                "CPU, GPU, NPU, memory, board, PCI and USB models, limits and identifiers",
                "Utilisation, clocks, temperatures, fans and power",
                "Disks: capacity, health, SMART counters and mount points",
                "Network interface names and byte counters",
                "The largest processes by name and memory",
                "Inference engine counters and agent session counts",
                "The provenance of every value (measured, derived, specification)",
            ],
            Self::DeviceProfile => vec![
                "Hardware identity: SMBIOS/platform UUID, board, BIOS, chassis and disk serials, \
                 CPU id, machine GUID or machine-id, physical MAC addresses",
                "Your account name, full name, home directory, shell, terminal, domain or \
                 workgroup, and whether you are an administrator",
                "Host name, OS version and build, kernel, locale, time zone, UI language",
                "CPU model, cores, clock and instruction-set features; RAM size, speed and modules",
                "Each GPU's vendor, model, VRAM, driver, PCI id; CUDA and ROCm versions",
                "Each disk's model, size and kind; display resolutions and refresh rates",
                "Chassis type, battery presence and capacity, virtual machine or container type",
                "The local Ollama version and the names of the models you have pulled",
                "Sent unhashed, once per `ironmon serve` session, only while System Telemetry is on",
            ],
            Self::BasicTelemetry => vec![
                "Application version",
                "Operating system type (Linux/Windows/macOS)",
                "Crash stack traces (anonymized)",
                "Error frequency (aggregated)",
            ],
            Self::HardwareInfo => vec![
                "GPU vendor (NVIDIA/AMD/Intel)",
                "GPU generation (e.g., RTX 30-series, not exact model)",
                "CPU architecture (x86_64/ARM)",
                "Approximate RAM size (e.g., 16GB range)",
            ],
            Self::PerformanceMetrics => vec![
                "Average GPU utilization",
                "Average memory usage",
                "Frame time distributions",
                "Application startup time",
            ],
            Self::DetailedDiagnostics => vec![
                "Exact GPU model and driver version",
                "Kernel version",
                "System configuration",
                "Running processes (names only, no PIDs or paths)",
            ],
            Self::Analytics => vec![
                "Feature usage frequency",
                "Tab/view navigation patterns",
                "Configuration changes (types, not values)",
                "Session duration",
            ],
        }
    }
}

/// Consent decision record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsentRecord {
    /// The scope this consent applies to
    pub scope: ConsentScope,

    /// Whether user granted consent
    pub granted: bool,

    /// When the consent was recorded
    pub timestamp: u64,

    /// Version of the consent prompt shown
    pub prompt_version: u32,

    /// Always `None`. No IP address is recorded, hashed or otherwise; the
    /// field exists only because it was declared, and `record_consent` has
    /// never populated it. Queued for removal, which is breaking.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_hash: Option<String>,
}

/// Consent management configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsentConfig {
    /// All consent records
    pub records: HashMap<ConsentScope, ConsentRecord>,

    /// When the user was first prompted
    pub first_prompt_timestamp: Option<u64>,

    /// Last time consent was reviewed
    pub last_review_timestamp: Option<u64>,

    /// User can set a reminder to review consent
    pub review_reminder_days: Option<u32>,

    /// Version of consent system (for tracking changes)
    pub consent_version: u32,
}

impl Default for ConsentConfig {
    fn default() -> Self {
        Self {
            records: HashMap::new(),
            first_prompt_timestamp: None,
            last_review_timestamp: None,
            review_reminder_days: Some(90), // Remind every 90 days by default
            consent_version: 1,
        }
    }
}

/// Consent manager - handles all consent-related operations
pub struct ConsentManager {
    config: ConsentConfig,
    config_path: PathBuf,
}

impl ConsentManager {
    /// Current version of the consent prompt/policy
    const CURRENT_PROMPT_VERSION: u32 = 1;

    /// Get default config path
    pub fn default_path() -> Result<PathBuf> {
        let config_dir = if cfg!(windows) {
            std::env::var("APPDATA")
                .map(PathBuf::from)
                .map_err(|_| IronError::Configuration("APPDATA not set".into()))?
        } else {
            std::env::var("HOME")
                .map(|h| PathBuf::from(h).join(".config"))
                .map_err(|_| IronError::Configuration("HOME not set".into()))?
        };

        Ok(config_dir.join("ironmon").join("consent.toml"))
    }

    /// A consent record left at the pre-rename path, when this install has none.
    ///
    /// **Deliberately not adopted.** `config.toml` and the profile audit log are
    /// copied forward by [`crate::legacy_paths::adopt`]; consent is not. Copying
    /// it would silently restore a privilege grant a user made to a program
    /// under a different name, and this crate's standing rule is that absent
    /// consent is asked for rather than assumed — the failure stays in the safe
    /// direction. What a caller can honestly do is *tell* the user the old file
    /// is there, which is what this reports.
    pub fn legacy_record() -> Option<std::path::PathBuf> {
        let current = Self::default_path().ok()?;
        crate::legacy_paths::legacy_consent_exists(&current)
    }

    /// Load consent configuration from default path
    pub fn load() -> Result<Self> {
        let path = Self::default_path()?;
        Self::load_from(&path)
    }

    /// Load consent configuration from specific path
    pub fn load_from(path: &PathBuf) -> Result<Self> {
        let config = if path.exists() {
            let contents = fs::read_to_string(path).map_err(|e| {
                IronError::Configuration(format!("Failed to read consent config: {}", e))
            })?;

            toml::from_str(&contents).map_err(|e| {
                IronError::Configuration(format!("Failed to parse consent config: {}", e))
            })?
        } else {
            ConsentConfig::default()
        };

        Ok(Self {
            config,
            config_path: path.clone(),
        })
    }

    /// Save consent configuration
    pub fn save(&self) -> Result<()> {
        // Create parent directory if it doesn't exist
        if let Some(parent) = self.config_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                IronError::Configuration(format!("Failed to create config directory: {}", e))
            })?;
        }

        let contents = toml::to_string_pretty(&self.config).map_err(|e| {
            IronError::Configuration(format!("Failed to serialize consent config: {}", e))
        })?;

        fs::write(&self.config_path, contents).map_err(|e| {
            IronError::Configuration(format!("Failed to write consent config: {}", e))
        })?;

        Ok(())
    }

    /// Check if user has granted consent for a specific scope
    ///
    /// # Sandbox Protection
    ///
    /// Returns `false` if running in a sandboxed environment, regardless of
    /// stored consent. This ensures no data is collected during analysis or testing.
    ///
    /// # Runtime Opt-Out
    ///
    /// Also returns `false` if the IRONMON_NO_TELEMETRY or IRONMON_OFFLINE environment
    /// variables are set (via --no-telemetry or --offline CLI flags).
    pub fn has_consent(&self, scope: ConsentScope) -> bool {
        // CRITICAL: runtime opt-outs first (--no-telemetry, --offline, DO_NOT_TRACK)
        if runtime_opt_out().is_some() {
            return false;
        }

        // CRITICAL: Never collect data in sandboxed environments
        let sandbox_detector = crate::sandbox::SandboxDetector::new();
        if sandbox_detector.is_sandboxed() {
            return false;
        }

        self.recorded_or_default(scope)
    }

    /// The recorded answer for `scope`, or its default when unanswered:
    /// granted for System Telemetry (opt-out), denied for every other scope.
    /// Ignores the environment and sandbox detection, which `has_consent` adds.
    /// Whether the user has answered for this scope at all.
    pub fn is_recorded(&self, scope: ConsentScope) -> bool {
        self.config.records.contains_key(&scope)
    }

    pub fn recorded_or_default(&self, scope: ConsentScope) -> bool {
        self.config
            .records
            .get(&scope)
            .map(|r| r.granted)
            .unwrap_or(scope.default_granted())
    }

    /// Whether data may be collected under `scope` right now.
    ///
    /// Combines consent, sandbox detection, the runtime opt-out flags, and
    /// whether a collector for the scope exists at all. Only System Telemetry
    /// has one — see [`ConsentScope::is_collected`].
    pub fn should_collect_data(&self, scope: ConsentScope) -> bool {
        // Nothing collects under this scope, so consent to it cannot make
        // collection appropriate. Checked first: it is the reason the answer
        // is false regardless of everything below.
        if !scope.is_collected() {
            return false;
        }

        // Check runtime opt-outs first (fastest check)
        if runtime_opt_out().is_some() {
            return false;
        }

        // Check sandbox second (fast check)
        let sandbox_detector = crate::sandbox::SandboxDetector::new();
        if sandbox_detector.is_sandboxed() {
            return false;
        }

        // Then check consent
        self.has_consent(scope)
    }

    /// Check if any consent prompt is needed (first run or consent expired)
    pub fn needs_consent_prompt(&self) -> bool {
        // First run - never prompted before
        if self.config.first_prompt_timestamp.is_none() {
            return true;
        }

        // Check if review reminder is due
        if let (Some(last_review), Some(reminder_days)) = (
            self.config.last_review_timestamp,
            self.config.review_reminder_days,
        ) {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();

            let days_since_review = (now - last_review) / 86400;
            if days_since_review >= reminder_days as u64 {
                return true;
            }
        }

        false
    }

    /// Record consent decision for a scope
    pub fn record_consent(&mut self, scope: ConsentScope, granted: bool) -> Result<()> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Update first prompt timestamp if this is the first time
        if self.config.first_prompt_timestamp.is_none() {
            self.config.first_prompt_timestamp = Some(timestamp);
        }

        // Update last review timestamp
        self.config.last_review_timestamp = Some(timestamp);

        // Create consent record
        let record = ConsentRecord {
            scope,
            granted,
            timestamp,
            prompt_version: Self::CURRENT_PROMPT_VERSION,
            ip_hash: None, // We don't collect IP addresses for consent
        };

        self.config.records.insert(scope, record);
        self.save()?;

        Ok(())
    }

    /// Revoke consent for a specific scope
    pub fn revoke_consent(&mut self, scope: ConsentScope) -> Result<()> {
        self.record_consent(scope, false)?;
        Ok(())
    }

    /// Revoke all consents
    pub fn revoke_all(&mut self) -> Result<()> {
        for scope in ConsentScope::ALL {
            self.revoke_consent(scope)?;
        }
        Ok(())
    }

    /// Get all consent records
    pub fn get_all_consents(&self) -> &HashMap<ConsentScope, ConsentRecord> {
        &self.config.records
    }

    /// Export consent status as human-readable string
    pub fn export_consent_status(&self) -> String {
        let mut output = String::from("=== IronMonitor - Consent Status ===\n\n");

        if let Some(first_prompt) = self.config.first_prompt_timestamp {
            output.push_str(&format!(
                "First prompted: {}\n",
                format_timestamp(first_prompt)
            ));
        }

        if let Some(last_review) = self.config.last_review_timestamp {
            output.push_str(&format!(
                "Last reviewed: {}\n",
                format_timestamp(last_review)
            ));
        }

        output.push_str(&format!(
            "\nConsent version: {}\n\n",
            self.config.consent_version
        ));

        output.push_str("Current consents:\n");
        output.push_str("─────────────────────────────────────────\n");

        for scope in ConsentScope::ALL {
            let status = if self.has_consent(scope) {
                "[+] GRANTED"
            } else {
                "[-] DENIED"
            };
            output.push_str(&format!("{:<30} {}\n", scope.name(), status));

            if let Some(record) = self.config.records.get(&scope) {
                output.push_str(&format!(
                    "  Recorded: {}\n",
                    format_timestamp(record.timestamp)
                ));
            }
        }

        output.push_str("\n=== End of Consent Status ===\n");
        output
    }

    /// Request consent interactively (CLI mode)
    pub fn request_consent(&mut self, scope: ConsentScope) -> Result<bool> {
        println!("\n╔════════════════════════════════════════════════════════════════╗");
        println!("║           IronMonitor - Data Collection Consent           ║");
        println!("╚════════════════════════════════════════════════════════════════╝\n");

        println!("Scope: {}\n", scope.name());
        println!("Description:\n{}\n", scope.description());
        println!("Data collected:");
        for point in scope.data_points() {
            println!("  * {}", point);
        }
        println!();
        println!("You can review and change your consent at any time by:");
        println!("  - Running: ironmon --consent-status");
        println!("  - Running: ironmon --revoke-consent");
        println!();

        loop {
            println!("Do you consent to this data collection? [y/N]: ");
            let mut input = String::new();
            let bytes = std::io::stdin()
                .read_line(&mut input)
                .map_err(|e| IronError::Configuration(format!("Failed to read input: {}", e)))?;

            // End of input is not an answer. It used to fall through to the
            // `""` arm below, which recorded a timestamped denial nobody gave,
            // and which also moved the review date forward and suppressed the
            // reminder. Leave the scope unrecorded instead.
            if bytes == 0 {
                return Err(IronError::Configuration(format!(
                    "no answer for {} on standard input; consent was not recorded",
                    scope.name()
                )));
            }

            let input = input.trim().to_lowercase();
            match input.as_str() {
                "y" | "yes" => {
                    self.record_consent(scope, true)?;
                    println!("[+] Consent granted for {}\n", scope.name());
                    return Ok(true);
                }
                "n" | "no" | "" => {
                    self.record_consent(scope, false)?;
                    println!("[-] Consent denied for {}\n", scope.name());
                    return Ok(false);
                }
                _ => {
                    println!("Invalid input. Please enter 'y' for yes or 'n' for no.");
                }
            }
        }
    }

    /// Request multiple consents interactively
    pub fn request_all_consents(&mut self) -> Result<()> {
        println!("\n╔════════════════════════════════════════════════════════════════╗");
        println!("║           Welcome to IronMonitor!                         ║");
        println!("╚════════════════════════════════════════════════════════════════╝\n");
        println!("System Telemetry is ON by default: `ironmon serve` sends its readings to");
        println!("telemetry.nervosys.ai. The other categories are off until you agree.\n");
        println!("You'll be asked about each. You can say 'no' to all of them -");
        println!("the tool works the same without any data collection.\n");

        println!("Press Enter to continue...");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).ok();

        for scope in [
            ConsentScope::SystemTelemetry,
            ConsentScope::DeviceProfile,
            ConsentScope::BasicTelemetry,
            ConsentScope::HardwareInfo,
            ConsentScope::PerformanceMetrics,
            ConsentScope::Analytics,
        ] {
            self.request_consent(scope)?;
        }

        println!("╔════════════════════════════════════════════════════════════════╗");
        println!("║           Consent Setup Complete                              ║");
        println!("╚════════════════════════════════════════════════════════════════╝\n");
        println!("You can review or change these settings at any time:");
        println!("  ironmon --consent-status      # View current consent status");
        println!("  ironmon --revoke-consent      # Revoke all consents");
        println!("  ironmon --consent-review      # Review and change specific consents\n");

        Ok(())
    }
}

/// Format timestamp as human-readable string
fn format_timestamp(timestamp: u64) -> String {
    // An unrepresentable timestamp used to fall back to the Unix epoch, which
    // displayed as 1970-01-01 and read like a real date. Say it is unknown.
    match chrono::DateTime::from_timestamp(timestamp as i64, 0) {
        Some(datetime) => datetime.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
        None => "unknown time".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_consent() {
        let manager = ConsentManager {
            config: ConsentConfig::default(),
            config_path: PathBuf::from("/tmp/test_consent.toml"),
        };

        // Test that default config has no explicit consent records
        assert!(manager.config.records.is_empty());
    }

    #[test]
    fn test_consent_recording() {
        let mut manager = ConsentManager {
            config: ConsentConfig::default(),
            config_path: PathBuf::from("/tmp/test_consent.toml"),
        };

        // Record consent
        manager
            .record_consent(ConsentScope::BasicTelemetry, true)
            .unwrap();

        // Check the record directly to bypass sandbox detection in has_consent
        // (sandbox detection may interfere with tests in CI/VMs)
        let record = manager.config.records.get(&ConsentScope::BasicTelemetry);
        assert!(record.is_some());
        assert!(record.unwrap().granted);

        // Revoke consent
        manager
            .revoke_consent(ConsentScope::BasicTelemetry)
            .unwrap();

        // Verify revocation by checking record directly
        let record = manager.config.records.get(&ConsentScope::BasicTelemetry);
        assert!(record.is_some());
        assert!(!record.unwrap().granted);
    }

    #[test]
    fn test_consent_scope_descriptions() {
        let scope = ConsentScope::BasicTelemetry;
        assert!(!scope.name().is_empty());
        assert!(!scope.description().is_empty());
        assert!(!scope.data_points().is_empty());
    }

    /// Every scope except System Telemetry, which is the one that collects.
    const UNIMPLEMENTED_SCOPES: [ConsentScope; 5] = [
        ConsentScope::BasicTelemetry,
        ConsentScope::HardwareInfo,
        ConsentScope::PerformanceMetrics,
        ConsentScope::DetailedDiagnostics,
        ConsentScope::Analytics,
    ];

    /// System Telemetry is the one collecting scope, and it is opt-out: granted
    /// with no record, refused once a denial is recorded. If either changes,
    /// the module docs, `ironmon privacy` and the `ironmon serve` banner say
    /// otherwise and must change with it.
    #[test]
    fn system_telemetry_is_collected_and_on_until_denied() {
        let mut manager = ConsentManager {
            config: ConsentConfig::default(),
            config_path: PathBuf::from("unused-in-this-test.toml"),
        };
        assert!(ConsentScope::SystemTelemetry.is_collected());
        assert!(ConsentScope::SystemTelemetry.default_granted());
        assert!(manager.recorded_or_default(ConsentScope::SystemTelemetry));
        manager.config.records.insert(
            ConsentScope::SystemTelemetry,
            ConsentRecord {
                scope: ConsentScope::SystemTelemetry,
                granted: false,
                timestamp: 1,
                prompt_version: 1,
                ip_hash: None,
            },
        );
        assert!(!manager.recorded_or_default(ConsentScope::SystemTelemetry));
        assert!(!manager.should_collect_data(ConsentScope::SystemTelemetry));
        assert_eq!(
            ConsentScope::ALL.len(),
            7,
            "a new scope needs a default and a screen"
        );
    }

    /// The disclosure in this module, and everything `ironmon privacy` prints,
    /// says that nothing is collected. This is what makes that a checked claim
    /// rather than a comment.
    ///
    /// It fails the moment a scope is marked implemented, which is exactly when
    /// the module docs, `ironmon privacy status`, `ironmon privacy info` and
    /// `ironmon privacy opt-in` all stop being true and need rewriting. Deleting
    /// this test to make a new collector pass would remove the only thing
    /// keeping those four in step.
    #[test]
    fn device_profile_is_collected_but_off_until_granted() {
        assert!(ConsentScope::DeviceProfile.is_collected());
        assert!(!ConsentScope::DeviceProfile.default_granted());
        let manager = ConsentManager {
            config: ConsentConfig::default(),
            config_path: std::path::PathBuf::from("unused.toml"),
        };
        assert!(!manager.recorded_or_default(ConsentScope::DeviceProfile));
        assert!(!manager.is_recorded(ConsentScope::DeviceProfile));
    }

    #[test]
    fn granting_an_unimplemented_scope_still_collects_nothing() {
        let mut manager = ConsentManager {
            config: ConsentConfig::default(),
            config_path: PathBuf::from("unused-in-this-test.toml"),
        };

        for scope in UNIMPLEMENTED_SCOPES {
            manager.config.records.insert(
                scope,
                ConsentRecord {
                    scope,
                    granted: true,
                    timestamp: 1,
                    prompt_version: 1,
                    ip_hash: None,
                },
            );
        }

        for scope in UNIMPLEMENTED_SCOPES {
            assert!(
                !scope.is_collected(),
                concat!(
                    "{} claims to be collected. If that is now true, update the ",
                    "module docs and the `ironmon privacy` screens, which state ",
                    "that only System Telemetry is sent."
                ),
                scope.name()
            );
            assert!(
                !manager.should_collect_data(scope),
                concat!(
                    "{} was granted and reported that it should collect, but ",
                    "no collector for it exists"
                ),
                scope.name()
            );
        }
    }

    /// A scope nobody has answered for is not a scope anyone agreed to.
    #[test]
    fn a_scope_with_no_record_is_not_granted() {
        let manager = ConsentManager {
            config: ConsentConfig::default(),
            config_path: PathBuf::from("unused-in-this-test.toml"),
        };

        assert!(
            manager.config.records.is_empty(),
            "the default config records no answers"
        );

        // `has_consent` consults the environment and the sandbox detector
        // first, either of which would return false for reasons unrelated to
        // the default. Read the default directly so the assertion is about
        // the default.
        let answered = manager
            .config
            .records
            .get(&ConsentScope::Analytics)
            .map(|r| r.granted)
            .unwrap_or(false);
        assert!(
            !answered,
            "an unanswered scope defaults to denied, not granted"
        );
    }
}
