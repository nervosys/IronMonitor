//! Configuration management for IronMonitor
//!
//! This module provides configuration persistence for TUI preferences,
//! display options, and monitoring settings.

use crate::error::{IronError, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// IronMonitor configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    /// General display options
    pub general: GeneralConfig,
    /// GPU-specific options
    pub gpu: GpuConfig,
    /// Process list options
    pub process: ProcessConfig,
    /// Chart/graph options
    pub chart: ChartConfig,
}

/// General display configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    /// Update interval in milliseconds
    #[serde(default = "default_update_interval")]
    pub update_interval_ms: u32,
    /// Use Fahrenheit instead of Celsius
    #[serde(default)]
    pub use_fahrenheit: bool,
    /// Color scheme: "default", "dark", "light", "nvtop"
    #[serde(default = "default_color_scheme")]
    pub color_scheme: String,
    /// Hide inactive encoders/decoders after timeout (seconds)
    #[serde(default = "default_encode_decode_timeout")]
    pub encode_decode_hiding_timer: u32,
}

/// GPU-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuConfig {
    /// List of GPU indices to monitor (empty = all)
    #[serde(default)]
    pub monitored_gpus: Vec<usize>,
    /// Show detailed GPU information
    #[serde(default = "default_true")]
    pub show_details: bool,
    /// Show GPU processes
    #[serde(default = "default_true")]
    pub show_processes: bool,
    /// Reverse plot direction
    #[serde(default)]
    pub reverse_plot: bool,
}

/// Process list configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessConfig {
    /// Visible columns
    #[serde(default = "default_process_columns")]
    pub visible_columns: Vec<String>,
    /// Sort column
    #[serde(default = "default_sort_column")]
    pub sort_column: String,
    /// Sort ascending (false = descending)
    #[serde(default)]
    pub sort_ascending: bool,
    /// Filter out nvtop/ironmon process
    #[serde(default = "default_true")]
    pub hide_self: bool,
}

/// Chart/graph configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartConfig {
    /// Metrics to display in charts (max 4)
    #[serde(default = "default_chart_metrics")]
    pub metrics: Vec<String>,
    /// Chart history length in seconds
    #[serde(default = "default_history_length")]
    pub history_length: u32,
}

// Default value functions
fn default_update_interval() -> u32 {
    1000 // 1 second
}

fn default_color_scheme() -> String {
    "default".to_string()
}

fn default_encode_decode_timeout() -> u32 {
    30 // 30 seconds
}

fn default_true() -> bool {
    true
}

fn default_process_columns() -> Vec<String> {
    vec![
        "pid".to_string(),
        "user".to_string(),
        "name".to_string(),
        "gpu".to_string(),
        "gpu_mem".to_string(),
        "cpu".to_string(),
        "mem".to_string(),
    ]
}

fn default_sort_column() -> String {
    "gpu_mem".to_string()
}

fn default_chart_metrics() -> Vec<String> {
    vec![
        "gpu_util".to_string(),
        "mem_util".to_string(),
        "temperature".to_string(),
        "power".to_string(),
    ]
}

fn default_history_length() -> u32 {
    60 // 60 seconds
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            update_interval_ms: default_update_interval(),
            use_fahrenheit: false,
            color_scheme: default_color_scheme(),
            encode_decode_hiding_timer: default_encode_decode_timeout(),
        }
    }
}

impl Default for GpuConfig {
    fn default() -> Self {
        Self {
            monitored_gpus: Vec::new(),
            show_details: true,
            show_processes: true,
            reverse_plot: false,
        }
    }
}

impl Default for ProcessConfig {
    fn default() -> Self {
        Self {
            visible_columns: default_process_columns(),
            sort_column: default_sort_column(),
            sort_ascending: false,
            hide_self: true,
        }
    }
}

impl Default for ChartConfig {
    fn default() -> Self {
        Self {
            metrics: default_chart_metrics(),
            history_length: default_history_length(),
        }
    }
}

impl Config {
    /// Get the default configuration file path
    ///
    /// Returns `~/.config/ironmon/config.toml` on Unix-like systems,
    /// or `%APPDATA%\ironmon\config.toml` on Windows.
    pub fn default_path() -> Result<PathBuf> {
        let config_dir = if cfg!(windows) {
            std::env::var("APPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("."))
        } else {
            std::env::var("HOME")
                .map(|home| PathBuf::from(home).join(".config"))
                .unwrap_or_else(|_| PathBuf::from(".config"))
        };

        Ok(config_dir.join("ironmon"))
    }

    /// Load configuration from the default path
    pub fn load() -> Result<Self> {
        let config_dir = Self::default_path()?;
        let config_file = config_dir.join("config.toml");

        // The rename moved this directory out from under an existing install.
        // Adopt the pre-rename file if this one is absent; the call is
        // idempotent and does nothing once a current file exists.
        let _ = crate::legacy_paths::adopt(&config_file);

        if !config_file.exists() {
            return Ok(Self::default());
        }

        let contents = std::fs::read_to_string(&config_file)?;
        let config: Config = toml::from_str(&contents)
            .map_err(|e| IronError::Parse(format!("Failed to parse config: {}", e)))?;

        Ok(config)
    }

    /// Load configuration from a specific path
    pub fn load_from(path: &PathBuf) -> Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&contents)
            .map_err(|e| IronError::Parse(format!("Failed to parse config: {}", e)))?;
        Ok(config)
    }

    /// Save configuration to the default path
    pub fn save(&self) -> Result<()> {
        let config_dir = Self::default_path()?;
        std::fs::create_dir_all(&config_dir)?;

        let config_file = config_dir.join("config.toml");
        let contents = toml::to_string_pretty(self)
            .map_err(|e| IronError::Other(format!("Failed to serialize config: {}", e)))?;

        std::fs::write(&config_file, contents)?;
        Ok(())
    }

    /// Save configuration to a specific path
    pub fn save_to(&self, path: &PathBuf) -> Result<()> {
        let contents = toml::to_string_pretty(self)
            .map_err(|e| IronError::Other(format!("Failed to serialize config: {}", e)))?;
        std::fs::write(path, contents)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.general.update_interval_ms, 1000);
        assert!(!config.general.use_fahrenheit);
        assert_eq!(config.general.encode_decode_hiding_timer, 30);
        assert!(config.gpu.show_details);
        assert!(config.gpu.show_processes);
        assert!(config.process.hide_self);
    }

    #[test]
    fn test_config_serialization() {
        let config = Config::default();
        let toml_str = toml::to_string(&config).unwrap();
        let deserialized: Config = toml::from_str(&toml_str).unwrap();

        assert_eq!(
            config.general.update_interval_ms,
            deserialized.general.update_interval_ms
        );
        assert_eq!(
            config.general.use_fahrenheit,
            deserialized.general.use_fahrenheit
        );
    }
}
