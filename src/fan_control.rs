//! Advanced Fan Monitoring and Control
//!
//! This module provides comprehensive fan monitoring capabilities including:
//! - PWM (Pulse Width Modulation) control
//! - RPM (Revolutions Per Minute) monitoring
//! - Fan profiles (quiet, cool, performance, manual)
//! - Thermal zone integration
//! - Multi-platform support (Linux hwmon, Windows WMI)
//!
//! # Example
//!
//! ```no_run
//! use ironmonlib::fan_control::{FanMonitor, FanProfile};
//!
//! let monitor = FanMonitor::new().unwrap();
//!
//! // List all fans
//! for fan in monitor.fans() {
//!     println!("Fan: {} - {}% @ {} RPM", fan.name, fan.speed_percent, fan.rpm.unwrap_or(0));
//! }
//!
//! // Set fan profile
//! monitor.set_profile("cpu_fan", FanProfile::Quiet).unwrap();
//! ```

use crate::error::{IronError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Fan profile presets
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FanProfile {
    /// Silent operation - minimum fan speed
    Silent,
    /// Quiet operation - balanced noise/cooling
    Quiet,
    /// Cool operation - prioritize cooling
    Cool,
    /// Performance - maximum cooling
    Performance,
    /// Manual - user-controlled speed
    Manual,
    /// Automatic - system-controlled based on temperature
    Auto,
    /// Temperature-controlled (legacy Jetson)
    TempControl,
}

impl std::fmt::Display for FanProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FanProfile::Silent => write!(f, "silent"),
            FanProfile::Quiet => write!(f, "quiet"),
            FanProfile::Cool => write!(f, "cool"),
            FanProfile::Performance => write!(f, "performance"),
            FanProfile::Manual => write!(f, "manual"),
            FanProfile::Auto => write!(f, "auto"),
            FanProfile::TempControl => write!(f, "temp_control"),
        }
    }
}

impl std::str::FromStr for FanProfile {
    type Err = IronError;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "silent" => Ok(FanProfile::Silent),
            "quiet" => Ok(FanProfile::Quiet),
            "cool" => Ok(FanProfile::Cool),
            "performance" => Ok(FanProfile::Performance),
            "manual" => Ok(FanProfile::Manual),
            "auto" | "automatic" => Ok(FanProfile::Auto),
            "temp_control" => Ok(FanProfile::TempControl),
            _ => Err(IronError::InvalidValue(format!(
                "Unknown fan profile: {}",
                s
            ))),
        }
    }
}

/// Fan control mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FanControlMode {
    /// PWM control (0-255 or 0-100%)
    Pwm,
    /// RPM target control
    Rpm,
    /// DC voltage control
    Dc,
    /// Automatic/thermal control
    Auto,
}

/// Fan type classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FanType {
    /// CPU cooler fan
    Cpu,
    /// GPU cooler fan
    Gpu,
    /// Case/chassis fan
    Case,
    /// Power supply fan
    Psu,
    /// Chipset fan
    Chipset,
    /// System/generic fan
    System,
    /// Unknown type
    Unknown,
}

impl std::fmt::Display for FanType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FanType::Cpu => write!(f, "CPU"),
            FanType::Gpu => write!(f, "GPU"),
            FanType::Case => write!(f, "Case"),
            FanType::Psu => write!(f, "PSU"),
            FanType::Chipset => write!(f, "Chipset"),
            FanType::System => write!(f, "System"),
            FanType::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Detailed fan information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FanInfo {
    /// Fan name/identifier
    pub name: String,
    /// Fan type
    pub fan_type: FanType,
    /// Current speed percentage (0-100)
    pub speed_percent: f32,
    /// Current PWM value (0-255)
    pub pwm_value: Option<u8>,
    /// Current RPM (if available)
    pub rpm: Option<u32>,
    /// Minimum RPM (if available)
    pub rpm_min: Option<u32>,
    /// Maximum RPM (if available)
    pub rpm_max: Option<u32>,
    /// Target RPM (if in RPM mode)
    pub rpm_target: Option<u32>,
    /// Current profile
    pub profile: FanProfile,
    /// Available profiles
    pub available_profiles: Vec<FanProfile>,
    /// Control mode
    pub control_mode: FanControlMode,
    /// Is fan controllable
    pub controllable: bool,
    /// PWM enable state (for hwmon)
    pub pwm_enable: Option<u8>,
    /// Linked thermal zone
    pub thermal_zone: Option<String>,
    /// Current linked temperature (°C)
    pub linked_temp_celsius: Option<f32>,
    /// Sysfs path (Linux)
    #[serde(skip)]
    pub sysfs_path: Option<PathBuf>,
}

impl FanInfo {
    /// Create a new FanInfo
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            fan_type: FanType::Unknown,
            speed_percent: 0.0,
            pwm_value: None,
            rpm: None,
            rpm_min: None,
            rpm_max: None,
            rpm_target: None,
            profile: FanProfile::Auto,
            available_profiles: vec![FanProfile::Auto],
            control_mode: FanControlMode::Auto,
            controllable: false,
            pwm_enable: None,
            thermal_zone: None,
            linked_temp_celsius: None,
            sysfs_path: None,
        }
    }

    /// Check if fan is running
    pub fn is_running(&self) -> bool {
        self.speed_percent > 0.0 || self.rpm.is_some_and(|r| r > 0)
    }

    /// Check if fan is at full speed
    pub fn is_full_speed(&self) -> bool {
        self.speed_percent >= 95.0
    }

    /// Check if fan might be stalled (RPM = 0 but PWM > 0)
    pub fn is_potentially_stalled(&self) -> bool {
        self.speed_percent > 10.0 && self.rpm == Some(0)
    }

    /// Get fan efficiency estimate (RPM per PWM%)
    pub fn efficiency(&self) -> Option<f32> {
        match (self.rpm, self.speed_percent) {
            (Some(rpm), speed) if speed > 5.0 => Some(rpm as f32 / speed),
            _ => None,
        }
    }
}

/// Fan curve point for custom curves
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct FanCurvePoint {
    /// Temperature threshold (°C)
    pub temp_celsius: f32,
    /// Fan speed percentage (0-100)
    pub speed_percent: f32,
}

/// Custom fan curve
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FanCurve {
    /// Curve name
    pub name: String,
    /// Curve points (must be sorted by temperature)
    pub points: Vec<FanCurvePoint>,
    /// Hysteresis (temperature drop before speed decreases)
    pub hysteresis: f32,
}

impl FanCurve {
    /// Create a default quiet curve
    pub fn quiet() -> Self {
        Self {
            name: "Quiet".to_string(),
            points: vec![
                FanCurvePoint {
                    temp_celsius: 30.0,
                    speed_percent: 20.0,
                },
                FanCurvePoint {
                    temp_celsius: 50.0,
                    speed_percent: 30.0,
                },
                FanCurvePoint {
                    temp_celsius: 60.0,
                    speed_percent: 50.0,
                },
                FanCurvePoint {
                    temp_celsius: 70.0,
                    speed_percent: 70.0,
                },
                FanCurvePoint {
                    temp_celsius: 80.0,
                    speed_percent: 90.0,
                },
                FanCurvePoint {
                    temp_celsius: 90.0,
                    speed_percent: 100.0,
                },
            ],
            hysteresis: 5.0,
        }
    }

    /// Create a default performance curve
    pub fn performance() -> Self {
        Self {
            name: "Performance".to_string(),
            points: vec![
                FanCurvePoint {
                    temp_celsius: 30.0,
                    speed_percent: 40.0,
                },
                FanCurvePoint {
                    temp_celsius: 40.0,
                    speed_percent: 50.0,
                },
                FanCurvePoint {
                    temp_celsius: 50.0,
                    speed_percent: 70.0,
                },
                FanCurvePoint {
                    temp_celsius: 60.0,
                    speed_percent: 85.0,
                },
                FanCurvePoint {
                    temp_celsius: 70.0,
                    speed_percent: 95.0,
                },
                FanCurvePoint {
                    temp_celsius: 75.0,
                    speed_percent: 100.0,
                },
            ],
            hysteresis: 3.0,
        }
    }

    /// Create a silent curve (aggressive throttling)
    pub fn silent() -> Self {
        Self {
            name: "Silent".to_string(),
            points: vec![
                FanCurvePoint {
                    temp_celsius: 40.0,
                    speed_percent: 0.0,
                },
                FanCurvePoint {
                    temp_celsius: 55.0,
                    speed_percent: 20.0,
                },
                FanCurvePoint {
                    temp_celsius: 65.0,
                    speed_percent: 40.0,
                },
                FanCurvePoint {
                    temp_celsius: 75.0,
                    speed_percent: 60.0,
                },
                FanCurvePoint {
                    temp_celsius: 85.0,
                    speed_percent: 80.0,
                },
                FanCurvePoint {
                    temp_celsius: 90.0,
                    speed_percent: 100.0,
                },
            ],
            hysteresis: 8.0,
        }
    }

    /// Calculate target speed for given temperature
    pub fn calculate_speed(&self, temp_celsius: f32) -> f32 {
        if self.points.is_empty() {
            return 100.0;
        }

        // Below minimum temperature
        if temp_celsius <= self.points[0].temp_celsius {
            return self.points[0].speed_percent;
        }

        // Above maximum temperature
        if temp_celsius >= self.points.last().unwrap().temp_celsius {
            return self.points.last().unwrap().speed_percent;
        }

        // Interpolate between points
        for i in 0..self.points.len() - 1 {
            let p1 = &self.points[i];
            let p2 = &self.points[i + 1];

            if temp_celsius >= p1.temp_celsius && temp_celsius <= p2.temp_celsius {
                let temp_range = p2.temp_celsius - p1.temp_celsius;
                let speed_range = p2.speed_percent - p1.speed_percent;
                let temp_offset = temp_celsius - p1.temp_celsius;

                return p1.speed_percent + (speed_range * temp_offset / temp_range);
            }
        }

        100.0
    }
}

/// Thermal zone information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThermalZone {
    /// Zone name
    pub name: String,
    /// Zone type (CPU, GPU, etc.)
    pub zone_type: String,
    /// Current temperature (°C)
    pub temp_celsius: f32,
    /// Trip points
    pub trip_points: Vec<TripPoint>,
    /// Cooling devices linked to this zone
    pub cooling_devices: Vec<String>,
    /// Policy type
    pub policy: Option<String>,
}

/// Thermal trip point
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TripPoint {
    /// Trip point type (active, passive, hot, critical)
    pub trip_type: String,
    /// Temperature threshold (°C)
    pub temp_celsius: f32,
    /// Hysteresis (°C)
    pub hysteresis: Option<f32>,
}

/// Fan monitoring and control
pub struct FanMonitor {
    /// Discovered fans
    fans: Vec<FanInfo>,
    /// Thermal zones
    thermal_zones: Vec<ThermalZone>,
    /// Last update time
    last_update: Instant,
    /// Custom fan curves
    custom_curves: HashMap<String, FanCurve>,
}

impl FanMonitor {
    /// Create a new fan monitor
    pub fn new() -> Result<Self> {
        let mut monitor = Self {
            fans: Vec::new(),
            thermal_zones: Vec::new(),
            last_update: Instant::now(),
            custom_curves: HashMap::new(),
        };

        monitor.discover_fans()?;
        monitor.discover_thermal_zones()?;

        Ok(monitor)
    }

    /// Get all discovered fans
    pub fn fans(&self) -> &[FanInfo] {
        &self.fans
    }

    /// Get fan by name
    pub fn get_fan(&self, name: &str) -> Option<&FanInfo> {
        self.fans.iter().find(|f| f.name == name)
    }

    /// Get all thermal zones
    pub fn thermal_zones(&self) -> &[ThermalZone] {
        &self.thermal_zones
    }

    /// Refresh fan data
    pub fn refresh(&mut self) -> Result<()> {
        self.discover_fans()?;
        self.last_update = Instant::now();
        Ok(())
    }

    /// Set fan speed (percentage 0-100)
    pub fn set_speed(&self, fan_name: &str, speed_percent: f32) -> Result<()> {
        if !(0.0..=100.0).contains(&speed_percent) {
            return Err(IronError::InvalidValue(format!(
                "Speed must be 0-100%, got {}",
                speed_percent
            )));
        }

        #[cfg(target_os = "linux")]
        {
            self.linux_set_fan_speed(fan_name, speed_percent)
        }

        #[cfg(target_os = "windows")]
        {
            self.windows_set_fan_speed(fan_name, speed_percent)
        }

        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            let _ = (fan_name, speed_percent);
            Err(IronError::UnsupportedPlatform(
                "Fan control not supported on this platform".to_string(),
            ))
        }
    }

    /// Set fan PWM value (0-255)
    pub fn set_pwm(&self, fan_name: &str, pwm_value: u8) -> Result<()> {
        let speed_percent = (pwm_value as f32 / 255.0) * 100.0;
        self.set_speed(fan_name, speed_percent)
    }

    /// Set fan profile
    pub fn set_profile(&self, fan_name: &str, profile: FanProfile) -> Result<()> {
        #[cfg(target_os = "linux")]
        {
            self.linux_set_fan_profile(fan_name, profile)
        }

        #[cfg(not(target_os = "linux"))]
        {
            let _ = (fan_name, profile);
            Err(IronError::UnsupportedPlatform(
                "Fan profile control not supported on this platform".to_string(),
            ))
        }
    }

    /// Set custom fan curve
    pub fn set_curve(&mut self, fan_name: &str, curve: FanCurve) {
        self.custom_curves.insert(fan_name.to_string(), curve);
    }

    /// Get custom fan curve
    pub fn get_curve(&self, fan_name: &str) -> Option<&FanCurve> {
        self.custom_curves.get(fan_name)
    }

    /// Apply fan curve based on current temperature
    pub fn apply_curve(&self, fan_name: &str, temp_celsius: f32) -> Result<()> {
        if let Some(curve) = self.custom_curves.get(fan_name) {
            let target_speed = curve.calculate_speed(temp_celsius);
            self.set_speed(fan_name, target_speed)?;
        }
        Ok(())
    }

    /// Get time since last update
    pub fn time_since_update(&self) -> Duration {
        self.last_update.elapsed()
    }

    /// Discover fans on the system
    fn discover_fans(&mut self) -> Result<()> {
        self.fans.clear();

        #[cfg(target_os = "linux")]
        {
            self.linux_discover_fans()?;
        }

        #[cfg(target_os = "windows")]
        {
            self.windows_discover_fans()?;
        }

        #[cfg(target_os = "macos")]
        {
            self.macos_discover_fans()?;
        }

        Ok(())
    }

    /// Discover thermal zones
    fn discover_thermal_zones(&mut self) -> Result<()> {
        self.thermal_zones.clear();

        #[cfg(target_os = "linux")]
        {
            self.linux_discover_thermal_zones()?;
        }

        Ok(())
    }

    // ==================== Linux Implementation ====================

    #[cfg(target_os = "linux")]
    fn linux_discover_fans(&mut self) -> Result<()> {
        use std::fs;

        let hwmon_path = std::path::Path::new("/sys/class/hwmon");
        if !hwmon_path.exists() {
            return Ok(());
        }

        for entry in fs::read_dir(hwmon_path)
            .map_err(|e| IronError::System(format!("Failed to read hwmon: {}", e)))?
        {
            let entry =
                entry.map_err(|e| IronError::System(format!("Failed to read entry: {}", e)))?;

            let path = entry.path();

            // Read device name
            let name_file = path.join("name");
            let device_name = if name_file.exists() {
                fs::read_to_string(&name_file)
                    .map(|s| s.trim().to_string())
                    .unwrap_or_else(|_| entry.file_name().to_string_lossy().to_string())
            } else {
                entry.file_name().to_string_lossy().to_string()
            };

            // Look for PWM files (pwm1, pwm2, etc.)
            for i in 1..=10 {
                let pwm_file = path.join(format!("pwm{}", i));
                if !pwm_file.exists() {
                    continue;
                }

                let fan_name = format!("{}_{}", device_name, i);
                let mut fan = FanInfo::new(&fan_name);
                fan.sysfs_path = Some(path.clone());

                // Read PWM value
                if let Ok(pwm_str) = fs::read_to_string(&pwm_file) {
                    if let Ok(pwm) = pwm_str.trim().parse::<u8>() {
                        fan.pwm_value = Some(pwm);
                        fan.speed_percent = (pwm as f32 / 255.0) * 100.0;
                    }
                }

                // Read PWM enable (control mode)
                let pwm_enable_file = path.join(format!("pwm{}_enable", i));
                if let Ok(enable_str) = fs::read_to_string(&pwm_enable_file) {
                    if let Ok(enable) = enable_str.trim().parse::<u8>() {
                        fan.pwm_enable = Some(enable);
                        fan.control_mode = match enable {
                            0 => FanControlMode::Dc,
                            1 => FanControlMode::Pwm,
                            2 => FanControlMode::Auto,
                            _ => FanControlMode::Auto,
                        };
                        fan.controllable = enable < 2;
                    }
                }

                // Read RPM (fan*_input)
                let rpm_file = path.join(format!("fan{}_input", i));
                if let Ok(rpm_str) = fs::read_to_string(&rpm_file) {
                    if let Ok(rpm) = rpm_str.trim().parse::<u32>() {
                        fan.rpm = Some(rpm);
                    }
                }

                // Read min/max RPM
                let rpm_min_file = path.join(format!("fan{}_min", i));
                if let Ok(rpm_str) = fs::read_to_string(&rpm_min_file) {
                    if let Ok(rpm) = rpm_str.trim().parse::<u32>() {
                        fan.rpm_min = Some(rpm);
                    }
                }

                let rpm_max_file = path.join(format!("fan{}_max", i));
                if let Ok(rpm_str) = fs::read_to_string(&rpm_max_file) {
                    if let Ok(rpm) = rpm_str.trim().parse::<u32>() {
                        fan.rpm_max = Some(rpm);
                    }
                }

                // Classify fan type
                fan.fan_type = classify_fan_type(&device_name, &fan_name);

                // Set available profiles
                fan.available_profiles = vec![FanProfile::Manual, FanProfile::Auto];

                self.fans.push(fan);
            }
        }

        // Also check Jetson-specific fan paths
        self.linux_discover_jetson_fans()?;

        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn linux_discover_jetson_fans(&mut self) -> Result<()> {
        use std::fs;

        // Jetson fan paths
        let jetson_paths = ["/sys/devices/pwm-fan", "/sys/kernel/debug/tegra_fan"];

        for base_path in &jetson_paths {
            let path = std::path::Path::new(base_path);
            if !path.exists() {
                continue;
            }

            let mut fan = FanInfo::new("jetson_fan");
            fan.fan_type = FanType::System;
            fan.sysfs_path = Some(path.to_path_buf());

            // Read target PWM
            let target_pwm = path.join("target_pwm");
            if let Ok(pwm_str) = fs::read_to_string(&target_pwm) {
                if let Ok(pwm) = pwm_str.trim().parse::<u32>() {
                    fan.pwm_value = Some((pwm.min(255)) as u8);
                    fan.speed_percent = (pwm as f32 / 255.0) * 100.0;
                }
            }

            // Read RPM
            let rpm_file = path.join("rpm_measured");
            if let Ok(rpm_str) = fs::read_to_string(&rpm_file) {
                if let Ok(rpm) = rpm_str.trim().parse::<u32>() {
                    fan.rpm = Some(rpm);
                }
            }

            // Check temp control
            let temp_control = path.join("temp_control");
            if temp_control.exists() {
                if let Ok(tc_str) = fs::read_to_string(&temp_control) {
                    fan.profile = if tc_str.trim() == "1" {
                        FanProfile::TempControl
                    } else {
                        FanProfile::Manual
                    };
                }
                fan.available_profiles = vec![FanProfile::Manual, FanProfile::TempControl];
            }

            fan.controllable = true;
            self.fans.push(fan);
        }

        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn linux_discover_thermal_zones(&mut self) -> Result<()> {
        use std::fs;

        let thermal_path = std::path::Path::new("/sys/class/thermal");
        if !thermal_path.exists() {
            return Ok(());
        }

        for entry in fs::read_dir(thermal_path)
            .map_err(|e| IronError::System(format!("Failed to read thermal: {}", e)))?
        {
            let entry =
                entry.map_err(|e| IronError::System(format!("Failed to read entry: {}", e)))?;

            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            if !name.starts_with("thermal_zone") {
                continue;
            }

            // Read zone type
            let zone_type = fs::read_to_string(path.join("type"))
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|_| "unknown".to_string());

            // Read temperature
            let temp = fs::read_to_string(path.join("temp"))
                .ok()
                .and_then(|s| s.trim().parse::<i32>().ok())
                .map(|t| t as f32 / 1000.0)
                .unwrap_or(0.0);

            // Read trip points
            let mut trip_points = Vec::new();
            for i in 0..10 {
                let trip_type_file = path.join(format!("trip_point_{}_type", i));
                let trip_temp_file = path.join(format!("trip_point_{}_temp", i));

                if !trip_type_file.exists() {
                    break;
                }

                let trip_type = fs::read_to_string(&trip_type_file)
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();

                let trip_temp = fs::read_to_string(&trip_temp_file)
                    .ok()
                    .and_then(|s| s.trim().parse::<i32>().ok())
                    .map(|t| t as f32 / 1000.0)
                    .unwrap_or(0.0);

                let hyst_file = path.join(format!("trip_point_{}_hyst", i));
                let hysteresis = fs::read_to_string(&hyst_file)
                    .ok()
                    .and_then(|s| s.trim().parse::<i32>().ok())
                    .map(|t| t as f32 / 1000.0);

                trip_points.push(TripPoint {
                    trip_type,
                    temp_celsius: trip_temp,
                    hysteresis,
                });
            }

            // Read policy
            let policy = fs::read_to_string(path.join("policy"))
                .map(|s| s.trim().to_string())
                .ok();

            self.thermal_zones.push(ThermalZone {
                name,
                zone_type,
                temp_celsius: temp,
                trip_points,
                cooling_devices: Vec::new(), // Could enumerate cdev* links
                policy,
            });
        }

        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn linux_set_fan_speed(&self, fan_name: &str, speed_percent: f32) -> Result<()> {
        use std::fs;

        let fan = self
            .fans
            .iter()
            .find(|f| f.name == fan_name)
            .ok_or_else(|| IronError::DeviceNotFound(format!("Fan '{}' not found", fan_name)))?;

        let path = fan
            .sysfs_path
            .as_ref()
            .ok_or_else(|| IronError::System("No sysfs path for fan".to_string()))?;

        // Calculate PWM value
        let pwm = ((speed_percent / 100.0) * 255.0).round() as u8;

        // Try different PWM file patterns
        let pwm_files = [path.join("pwm1"), path.join("target_pwm")];

        for pwm_file in &pwm_files {
            if pwm_file.exists() {
                fs::write(pwm_file, format!("{}", pwm)).map_err(|e| {
                    IronError::System(format!("Failed to write PWM (need root?): {}", e))
                })?;
                return Ok(());
            }
        }

        Err(IronError::System("No writable PWM file found".to_string()))
    }

    #[cfg(target_os = "linux")]
    fn linux_set_fan_profile(&self, fan_name: &str, profile: FanProfile) -> Result<()> {
        use std::fs;

        let fan = self
            .fans
            .iter()
            .find(|f| f.name == fan_name)
            .ok_or_else(|| IronError::DeviceNotFound(format!("Fan '{}' not found", fan_name)))?;

        let path = fan
            .sysfs_path
            .as_ref()
            .ok_or_else(|| IronError::System("No sysfs path for fan".to_string()))?;

        // For standard hwmon, set pwm_enable
        // 0 = DC mode
        // 1 = PWM/manual mode
        // 2 = automatic mode
        let pwm_enable_file = path.join("pwm1_enable");
        if pwm_enable_file.exists() {
            let enable_value = match profile {
                FanProfile::Manual => 1,
                FanProfile::Auto
                | FanProfile::Quiet
                | FanProfile::Cool
                | FanProfile::Performance => 2,
                FanProfile::Silent => 1, // Manual with low speed
                FanProfile::TempControl => 2,
            };

            fs::write(&pwm_enable_file, format!("{}", enable_value)).map_err(|e| {
                IronError::System(format!("Failed to set fan profile (need root?): {}", e))
            })?;

            return Ok(());
        }

        // For Jetson temp_control
        let temp_control = path.join("temp_control");
        if temp_control.exists() {
            let value = match profile {
                FanProfile::TempControl | FanProfile::Auto => "1",
                FanProfile::Manual => "0",
                _ => "1",
            };

            fs::write(&temp_control, value)
                .map_err(|e| IronError::System(format!("Failed to set temp control: {}", e)))?;

            return Ok(());
        }

        Err(IronError::UnsupportedPlatform(
            "Fan profile control not available for this fan".to_string(),
        ))
    }

    // ==================== Windows Implementation ====================

    #[cfg(target_os = "windows")]
    fn windows_discover_fans(&mut self) -> Result<()> {
        use wmi::{COMLibrary, WMIConnection};

        // WMI Win32_Fan structure (rarely populated on consumer hardware)
        #[derive(serde::Deserialize, Debug)]
        #[serde(rename_all = "PascalCase")]
        #[allow(dead_code)]
        struct Win32Fan {
            device_id: Option<String>,
            name: Option<String>,
            active_cooling: Option<bool>,
            status: Option<String>,
        }

        // Try to query Win32_Fan
        if let Ok(com) = COMLibrary::new() {
            if let Ok(wmi) = WMIConnection::new(com) {
                let fans: Vec<Win32Fan> = wmi.query().unwrap_or_default();

                for wmi_fan in fans {
                    let name = wmi_fan
                        .name
                        .clone()
                        .or(wmi_fan.device_id.clone())
                        .unwrap_or_else(|| "System Fan".to_string());

                    let mut fan = FanInfo::new(&name);
                    fan.fan_type = FanType::System;
                    fan.controllable = false; // WMI doesn't support control

                    // Win32_Fan exposes no tachometer reading, so `rpm` stays None.
                    //
                    // This previously reported a hardcoded 1000 RPM whenever
                    // ActiveCooling was true. That is indistinguishable from a real
                    // measurement on screen, and a fan pinned at exactly 1000 RPM
                    // while temperatures climb is actively misleading. An unknown
                    // speed is better represented as unknown.
                    //
                    // ActiveCooling only says the fan is participating in cooling,
                    // which FanInfo has no field to express, so it is not recorded.
                    self.fans.push(fan);
                }
            }
        }

        // Also try WMI CIM_Fan (more generic)
        self.windows_discover_cim_fans();

        // Try to detect thermal zones via WMI
        self.windows_discover_thermal_zones();

        Ok(())
    }

    #[cfg(target_os = "windows")]
    fn windows_discover_cim_fans(&mut self) {
        use wmi::{COMLibrary, WMIConnection};

        #[derive(serde::Deserialize, Debug)]
        #[serde(rename_all = "PascalCase")]
        #[allow(dead_code)]
        struct CimFan {
            device_id: Option<String>,
            name: Option<String>,
            description: Option<String>,
        }

        if let Ok(com) = COMLibrary::new() {
            if let Ok(wmi) = WMIConnection::new(com) {
                // Query CIM_Fan
                let query = "SELECT * FROM CIM_Fan";
                if let Ok(fans) = wmi.raw_query::<CimFan>(query) {
                    for cim_fan in fans {
                        // Skip if we already have this fan
                        let name = cim_fan
                            .name
                            .clone()
                            .or(cim_fan.device_id.clone())
                            .unwrap_or_else(|| "CIM Fan".to_string());

                        if !self.fans.iter().any(|f| f.name == name) {
                            let mut fan = FanInfo::new(&name);
                            fan.fan_type = FanType::System;
                            fan.controllable = false;
                            self.fans.push(fan);
                        }
                    }
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    fn windows_discover_thermal_zones(&mut self) {
        use wmi::{COMLibrary, WMIConnection};

        // MSAcpi_ThermalZoneTemperature from root\WMI namespace
        #[derive(serde::Deserialize, Debug)]
        #[serde(rename = "MSAcpi_ThermalZoneTemperature")]
        #[serde(rename_all = "PascalCase")]
        #[allow(dead_code)]
        struct MsAcpiThermalZone {
            instance_name: Option<String>,
            current_temperature: Option<u32>, // Tenths of Kelvin
            active: Option<bool>,
            critical_trip_point: Option<u32>,
            passive_trip_point: Option<u32>,
        }

        if let Ok(com) = COMLibrary::new() {
            // Connect to root\WMI namespace
            if let Ok(wmi) = WMIConnection::with_namespace_path("root\\WMI", com) {
                if let Ok(zones) = wmi.query::<MsAcpiThermalZone>() {
                    for (idx, zone) in zones.iter().enumerate() {
                        let name = zone
                            .instance_name
                            .clone()
                            .unwrap_or_else(|| format!("thermal_zone{}", idx));

                        let mut tz = ThermalZone {
                            name: format!("zone{}", idx),
                            zone_type: name.clone(),
                            temp_celsius: 0.0,
                            trip_points: Vec::new(),
                            cooling_devices: Vec::new(),
                            policy: None,
                        };

                        // Convert from tenths of Kelvin to Celsius
                        if let Some(temp_tenths_k) = zone.current_temperature {
                            let temp_c = (temp_tenths_k as f32 / 10.0) - 273.15;
                            tz.temp_celsius = temp_c;
                        }

                        // Add trip points if available
                        if let Some(crit) = zone.critical_trip_point {
                            let crit_c = (crit as f32 / 10.0) - 273.15;
                            tz.trip_points.push(TripPoint {
                                trip_type: "critical".to_string(),
                                temp_celsius: crit_c,
                                hysteresis: None,
                            });
                        }

                        if let Some(passive) = zone.passive_trip_point {
                            let passive_c = (passive as f32 / 10.0) - 273.15;
                            tz.trip_points.push(TripPoint {
                                trip_type: "passive".to_string(),
                                temp_celsius: passive_c,
                                hysteresis: None,
                            });
                        }

                        self.thermal_zones.push(tz);
                    }
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    fn windows_set_fan_speed(&self, _fan_name: &str, _speed_percent: f32) -> Result<()> {
        Err(IronError::UnsupportedPlatform(
            "Fan control on Windows requires vendor-specific tools or OpenHardwareMonitor"
                .to_string(),
        ))
    }

    // ==================== macOS Implementation ====================

    #[cfg(target_os = "macos")]
    fn macos_discover_fans(&mut self) -> Result<()> {
        use std::process::Command;

        // Use SMC to read fan info (requires smcFanControl or similar)
        // This is a simplified version

        let output = Command::new("system_profiler")
            .args(["SPHardwareDataType"])
            .output();

        if let Ok(output) = output {
            if output.status.success() {
                // Macs typically have multiple fans
                for i in 1..=4 {
                    let mut fan = FanInfo::new(format!("fan{}", i));
                    fan.fan_type = FanType::System;
                    // SMC values would need to be read via IOKit
                    fan.controllable = false;
                    self.fans.push(fan);
                }
            }
        }

        Ok(())
    }
}

/// Classify fan type based on device/fan name
#[allow(dead_code)]
fn classify_fan_type(device_name: &str, fan_name: &str) -> FanType {
    let name_lower = format!("{} {}", device_name, fan_name).to_lowercase();

    if name_lower.contains("cpu") || name_lower.contains("processor") {
        FanType::Cpu
    } else if name_lower.contains("gpu")
        || name_lower.contains("nvidia")
        || name_lower.contains("amd")
    {
        FanType::Gpu
    } else if name_lower.contains("psu") || name_lower.contains("power") {
        FanType::Psu
    } else if name_lower.contains("case")
        || name_lower.contains("chassis")
        || name_lower.contains("sys")
    {
        FanType::Case
    } else if name_lower.contains("chip") || name_lower.contains("pch") {
        FanType::Chipset
    } else {
        FanType::Unknown
    }
}

/// Summary of fan status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FanSummary {
    /// Total number of fans
    pub total_fans: usize,
    /// Number of running fans
    pub running_fans: usize,
    /// Average speed across all fans
    pub avg_speed_percent: f32,
    /// Maximum RPM detected
    pub max_rpm: Option<u32>,
    /// Any stalled fans detected
    pub stalled_fans: Vec<String>,
    /// Fans at full speed
    pub full_speed_fans: Vec<String>,
}

/// Get a summary of all fans
pub fn fan_summary() -> Result<FanSummary> {
    let monitor = FanMonitor::new()?;
    let fans = monitor.fans();

    let running_fans: Vec<_> = fans.iter().filter(|f| f.is_running()).collect();
    let full_speed: Vec<_> = fans.iter().filter(|f| f.is_full_speed()).collect();
    let stalled: Vec<_> = fans.iter().filter(|f| f.is_potentially_stalled()).collect();

    let total_speed: f32 = fans.iter().map(|f| f.speed_percent).sum();
    let avg_speed = if fans.is_empty() {
        0.0
    } else {
        total_speed / fans.len() as f32
    };

    let max_rpm = fans.iter().filter_map(|f| f.rpm).max();

    Ok(FanSummary {
        total_fans: fans.len(),
        running_fans: running_fans.len(),
        avg_speed_percent: avg_speed,
        max_rpm,
        stalled_fans: stalled.iter().map(|f| f.name.clone()).collect(),
        full_speed_fans: full_speed.iter().map(|f| f.name.clone()).collect(),
    })
}

/// Get list of all fans (convenience function)
pub fn list_fans() -> Result<Vec<FanInfo>> {
    let monitor = FanMonitor::new()?;
    Ok(monitor.fans.clone())
}

/// Get list of thermal zones
pub fn list_thermal_zones() -> Result<Vec<ThermalZone>> {
    let monitor = FanMonitor::new()?;
    Ok(monitor.thermal_zones.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    // === FanProfile tests ===

    #[test]
    fn test_fan_profile_display() {
        assert_eq!(FanProfile::Silent.to_string(), "silent");
        assert_eq!(FanProfile::Quiet.to_string(), "quiet");
        assert_eq!(FanProfile::Cool.to_string(), "cool");
        assert_eq!(FanProfile::Performance.to_string(), "performance");
        assert_eq!(FanProfile::Manual.to_string(), "manual");
        assert_eq!(FanProfile::Auto.to_string(), "auto");
        assert_eq!(FanProfile::TempControl.to_string(), "temp_control");
    }

    #[test]
    fn test_fan_profile_from_str() {
        assert_eq!("silent".parse::<FanProfile>().unwrap(), FanProfile::Silent);
        assert_eq!("quiet".parse::<FanProfile>().unwrap(), FanProfile::Quiet);
        assert_eq!("cool".parse::<FanProfile>().unwrap(), FanProfile::Cool);
        assert_eq!(
            "performance".parse::<FanProfile>().unwrap(),
            FanProfile::Performance
        );
        assert_eq!("auto".parse::<FanProfile>().unwrap(), FanProfile::Auto);
        assert_eq!("automatic".parse::<FanProfile>().unwrap(), FanProfile::Auto);
        assert!("invalid".parse::<FanProfile>().is_err());
    }

    #[test]
    fn test_fan_profile_case_insensitive() {
        assert_eq!("QUIET".parse::<FanProfile>().unwrap(), FanProfile::Quiet);
        assert_eq!("Silent".parse::<FanProfile>().unwrap(), FanProfile::Silent);
    }

    // === FanType tests ===

    #[test]
    fn test_fan_type_display() {
        assert_eq!(FanType::Cpu.to_string(), "CPU");
        assert_eq!(FanType::Gpu.to_string(), "GPU");
        assert_eq!(FanType::Case.to_string(), "Case");
        assert_eq!(FanType::Unknown.to_string(), "Unknown");
    }

    // === FanInfo tests ===

    #[test]
    fn test_fan_info_new_defaults() {
        let fan = FanInfo::new("cpu_fan");
        assert_eq!(fan.name, "cpu_fan");
        assert_eq!(fan.fan_type, FanType::Unknown);
        assert_eq!(fan.speed_percent, 0.0);
        assert!(!fan.controllable);
    }

    #[test]
    fn test_fan_is_running_by_speed() {
        let mut fan = FanInfo::new("test");
        assert!(!fan.is_running());
        fan.speed_percent = 50.0;
        assert!(fan.is_running());
    }

    #[test]
    fn test_fan_is_running_by_rpm() {
        let mut fan = FanInfo::new("test");
        fan.rpm = Some(1000);
        assert!(fan.is_running());
    }

    #[test]
    fn test_fan_is_full_speed() {
        let mut fan = FanInfo::new("test");
        fan.speed_percent = 94.0;
        assert!(!fan.is_full_speed());
        fan.speed_percent = 95.0;
        assert!(fan.is_full_speed());
        fan.speed_percent = 100.0;
        assert!(fan.is_full_speed());
    }

    #[test]
    fn test_fan_is_potentially_stalled() {
        let mut fan = FanInfo::new("test");
        // Not stalled: speed 0, no rpm
        assert!(!fan.is_potentially_stalled());
        // Not stalled: speed > 10 but rpm not zero
        fan.speed_percent = 50.0;
        fan.rpm = Some(1200);
        assert!(!fan.is_potentially_stalled());
        // Stalled: speed > 10 and rpm == 0
        fan.rpm = Some(0);
        assert!(fan.is_potentially_stalled());
        // Not stalled: low speed
        fan.speed_percent = 5.0;
        assert!(!fan.is_potentially_stalled());
    }

    #[test]
    fn test_fan_efficiency() {
        let mut fan = FanInfo::new("test");
        // No rpm -> None
        assert!(fan.efficiency().is_none());
        // Low speed -> None
        fan.speed_percent = 3.0;
        fan.rpm = Some(500);
        assert!(fan.efficiency().is_none());
        // Normal case
        fan.speed_percent = 50.0;
        fan.rpm = Some(1000);
        let eff = fan.efficiency().unwrap();
        assert!((eff - 20.0).abs() < 0.01); // 1000 / 50 = 20
    }

    // === FanCurve tests ===

    #[test]
    fn test_fan_curve_quiet_has_points() {
        let curve = FanCurve::quiet();
        assert_eq!(curve.name, "Quiet");
        assert!(!curve.points.is_empty());
        assert!(curve.hysteresis > 0.0);
    }

    #[test]
    fn test_fan_curve_performance_has_points() {
        let curve = FanCurve::performance();
        assert_eq!(curve.name, "Performance");
        assert!(!curve.points.is_empty());
    }

    #[test]
    fn test_fan_curve_silent_has_points() {
        let curve = FanCurve::silent();
        assert_eq!(curve.name, "Silent");
        assert!(!curve.points.is_empty());
    }

    #[test]
    fn test_fan_curve_calculate_speed_below_min() {
        let curve = FanCurve::quiet();
        // Below minimum temp should return first point's speed
        let speed = curve.calculate_speed(0.0);
        assert_eq!(speed, curve.points[0].speed_percent);
    }

    #[test]
    fn test_fan_curve_calculate_speed_above_max() {
        let curve = FanCurve::quiet();
        let speed = curve.calculate_speed(200.0);
        assert_eq!(speed, curve.points.last().unwrap().speed_percent);
    }

    #[test]
    fn test_fan_curve_calculate_speed_interpolation() {
        let curve = FanCurve::quiet();
        // Quiet curve: 30°C=20%, 50°C=30%
        // At 40°C: 20 + (30-20)*(40-30)/(50-30) = 20 + 10*10/20 = 25
        let speed = curve.calculate_speed(40.0);
        assert!((speed - 25.0).abs() < 0.1);
    }

    #[test]
    fn test_fan_curve_calculate_speed_at_point() {
        let curve = FanCurve::quiet();
        // At exact point temperature
        let speed = curve.calculate_speed(30.0);
        assert_eq!(speed, 20.0);
    }

    #[test]
    fn test_fan_curve_empty_returns_100() {
        let curve = FanCurve {
            name: "empty".to_string(),
            points: vec![],
            hysteresis: 0.0,
        };
        assert_eq!(curve.calculate_speed(50.0), 100.0);
    }
}
