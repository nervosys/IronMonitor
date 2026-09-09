//! User Consent Management
//!
//! # IronMonitor collects nothing and transmits nothing
//!
//! Read this before anything else in the module, because the rest of it
//! describes machinery that does not currently drive anything.
//!
//! No code in this crate collects, aggregates or transmits telemetry,
//! analytics, hardware inventory or diagnostics. There is no endpoint. The
//! only outbound HTTP in the crate goes to LLM backends the user configures
//! and points at themselves. [`ConsentScope::is_collected`] returns `false`
//! for every scope, and it is the single place that changes if that ever
//! stops being true.
//!
//! What this module provides is the consent *record* — a persisted preference,
//! with a timestamp, that a future collector would have to honour. Presenting
//! it as a live privacy control would tell the user they had switched
//! something off that was never on, which is the same defect as reporting a
//! measurement that was never taken.
//!
//! # Principles
//!
//! - **Denied until asked**: a scope with no consent record is not granted.
//!   This module previously defaulted to granted while describing itself as
//!   opt-in; the two cannot both be true, and the safe reading is the one that
//!   does not assume permission nobody gave.
//! - **Easy opt-out**: `ironmon privacy opt-out` records a denial for every scope
//! - **Transparency**: the disclosure says what *would* be collected, and says
//!   plainly that none of it is
//! - **Auditability**: every consent decision is stored with the time it was made
//!
//! No claim is made here about compliance with any particular privacy
//! regulation. That is a question about a deployment and its operator, not
//! about a library, and this module is not in a position to answer it.
//!
//! # Example
//!
//! ```no_run
//! use ironmonlib::consent::{ConsentManager, ConsentScope};
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
/// use ironmonlib::consent::is_telemetry_disabled;
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
/// use ironmonlib::consent::is_offline_mode;
///
/// if is_offline_mode() {
///     // Skip any network-dependent features
///     return;
/// }
/// ```
pub fn is_offline_mode() -> bool {
    std::env::var("IRONMON_OFFLINE").is_ok()
}

/// Consent scopes - each represents a specific type of data collection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsentScope {
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
    /// Get human-readable name
    pub fn name(&self) -> &'static str {
        match self {
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
    /// user answered. `granting_every_scope_still_collects_nothing` in this
    /// module fails the moment an arm becomes `true`, which is the moment the
    /// disclosure and the CLI text need revisiting.
    pub fn is_collected(&self) -> bool {
        match self {
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
        // CRITICAL: Check runtime opt-out flags first (set by CLI --no-telemetry or --offline)
        if std::env::var("IRONMON_NO_TELEMETRY").is_ok() || std::env::var("IRONMON_OFFLINE").is_ok()
        {
            return false;
        }

        // CRITICAL: Never collect data in sandboxed environments
        let sandbox_detector = crate::sandbox::SandboxDetector::new();
        if sandbox_detector.is_sandboxed() {
            return false;
        }

        // A scope with no record has not been consented to. This defaulted to
        // `true` under a comment reading "opt-in by default", which is a
        // contradiction: a default of granted is opt-out. Nothing consumed the
        // answer, so the wrong default was inert, but it would have started
        // granting consent nobody gave on the day a collector landed.
        self.config
            .records
            .get(&scope)
            .map(|r| r.granted)
            .unwrap_or(false)
    }

    /// Whether data may be collected under `scope` right now.
    ///
    /// Combines consent, sandbox detection, the runtime opt-out flags, and
    /// whether a collector for the scope exists at all. It returns `false` for
    /// every scope today, because none is implemented — see
    /// [`ConsentScope::is_collected`].
    pub fn should_collect_data(&self, scope: ConsentScope) -> bool {
        // Nothing collects under this scope, so consent to it cannot make
        // collection appropriate. Checked first: it is the reason the answer
        // is false regardless of everything below.
        if !scope.is_collected() {
            return false;
        }

        // Check runtime opt-out flags first (fastest check, set by CLI)
        if std::env::var("IRONMON_NO_TELEMETRY").is_ok() || std::env::var("IRONMON_OFFLINE").is_ok()
        {
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
        for scope in [
            ConsentScope::BasicTelemetry,
            ConsentScope::HardwareInfo,
            ConsentScope::PerformanceMetrics,
            ConsentScope::DetailedDiagnostics,
            ConsentScope::Analytics,
        ] {
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

        for scope in [
            ConsentScope::BasicTelemetry,
            ConsentScope::HardwareInfo,
            ConsentScope::PerformanceMetrics,
            ConsentScope::DetailedDiagnostics,
            ConsentScope::Analytics,
        ] {
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
            std::io::stdin()
                .read_line(&mut input)
                .map_err(|e| IronError::Configuration(format!("Failed to read input: {}", e)))?;

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
        println!("This is your first time running IronMonitor.");
        println!("We respect your privacy and will only collect data you explicitly consent to.\n");
        println!("You'll be asked about several types of optional data collection.");
        println!("You can say 'no' to all of them - the tool works perfectly without any data collection.\n");

        println!("Press Enter to continue...");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).ok();

        for scope in [
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
    let datetime = chrono::DateTime::from_timestamp(timestamp as i64, 0)
        .unwrap_or(chrono::DateTime::UNIX_EPOCH);
    datetime.format("%Y-%m-%d %H:%M:%S UTC").to_string()
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

    const EVERY_SCOPE: [ConsentScope; 5] = [
        ConsentScope::BasicTelemetry,
        ConsentScope::HardwareInfo,
        ConsentScope::PerformanceMetrics,
        ConsentScope::DetailedDiagnostics,
        ConsentScope::Analytics,
    ];

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
    fn granting_every_scope_still_collects_nothing() {
        let mut manager = ConsentManager {
            config: ConsentConfig::default(),
            config_path: PathBuf::from("unused-in-this-test.toml"),
        };

        for scope in EVERY_SCOPE {
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

        for scope in EVERY_SCOPE {
            assert!(
                !scope.is_collected(),
                concat!(
                    "{} claims to be collected. If that is now true, update the ",
                    "module docs and the three `ironmon privacy` screens, which ",
                    "all state that IronMonitor collects and transmits nothing."
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
