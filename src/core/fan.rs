//! Fan monitoring and control

use crate::error::{IronError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Fan information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FanInfo {
    /// Fan speeds (percentage 0-100)
    pub speed: Vec<u32>,
    /// Fan RPM (optional)
    pub rpm: Option<Vec<u32>>,
    /// Fan profile
    pub profile: String,
    /// Fan governor (Jetson JP5+)
    pub governor: Option<String>,
    /// Controller type (Jetson JP5+)
    pub control: Option<String>,
}

/// Fan statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FanStats {
    /// Fans by name
    fans: HashMap<String, FanInfo>,
}

impl FanStats {
    /// Create a new fan stats instance
    pub fn new() -> Self {
        Self {
            fans: HashMap::new(),
        }
    }

    /// Get all fans
    pub fn fans(&self) -> &HashMap<String, FanInfo> {
        &self.fans
    }

    /// Get fan by name
    pub fn get_fan(&self, name: &str) -> Option<&FanInfo> {
        self.fans.get(name)
    }

    /// Get mutable reference to fans
    pub fn fans_mut(&mut self) -> &mut HashMap<String, FanInfo> {
        &mut self.fans
    }

    /// Set fan speed
    pub fn set_speed(&mut self, name: &str, speed: u32, index: usize) -> Result<()> {
        if speed > 100 {
            return Err(IronError::InvalidValue(format!(
                "Fan speed must be 0-100, got {}",
                speed
            )));
        }

        if !self.fans.contains_key(name) {
            return Err(IronError::DeviceNotFound(format!(
                "Fan '{}' not found",
                name
            )));
        }

        #[cfg(target_os = "linux")]
        {
            use crate::platform::linux::jetson::set_fan_speed;
            set_fan_speed(name, speed, index)
        }

        #[cfg(not(target_os = "linux"))]
        {
            let _ = index; // Suppress unused warning
            Err(IronError::UnsupportedPlatform(
                "Fan control not yet implemented on Windows".to_string(),
            ))
        }
    }

    /// Set fan profile (Jetson JP5+)
    pub fn set_profile(&mut self, name: &str, profile: &str) -> Result<()> {
        if !self.fans.contains_key(name) {
            return Err(IronError::DeviceNotFound(format!(
                "Fan '{}' not found",
                name
            )));
        }

        #[cfg(target_os = "linux")]
        {
            use crate::platform::linux::jetson::set_fan_profile;
            set_fan_profile(name, profile)
        }

        #[cfg(not(target_os = "linux"))]
        {
            let _ = profile; // Suppress unused warning
            Err(IronError::UnsupportedPlatform(
                "Fan profile control only available on Linux Jetson devices".to_string(),
            ))
        }
    }
}

impl Default for FanStats {
    fn default() -> Self {
        Self::new()
    }
}
