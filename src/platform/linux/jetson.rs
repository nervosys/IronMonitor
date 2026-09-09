//! Jetson-specific implementations

use crate::error::{IronError, Result};
use crate::platform::common::*;
use std::fs;
use std::path::{Path, PathBuf};

const JETSON_DEVFREQ_PATH: &str = "/sys/class/devfreq";
const KNOWN_GPU_NAMES: &[&str] = &["gv11b", "gp10b", "ga10b", "gb10b", "gpu"];

/// Check if running on a Jetson device
pub fn is_jetson() -> bool {
    // Check for Jetson-specific files
    path_exists("/sys/firmware/devicetree/base/model")
        && (path_exists("/dev/nvhost-gpu") || path_exists("/dev/nvhost-power-gpu"))
}

/// Detect Jetson GPU devices
pub fn find_jetson_gpus() -> Result<Vec<(String, PathBuf)>> {
    let mut gpus = Vec::new();

    if !Path::new(JETSON_DEVFREQ_PATH).is_dir() {
        return Ok(gpus);
    }

    for entry in fs::read_dir(JETSON_DEVFREQ_PATH)? {
        let entry = entry?;
        let path = entry.path();

        if !path.is_dir() && !path.is_symlink() {
            continue;
        }

        let name_path = path.join("device/of_node/name");
        if !name_path.is_file() {
            continue;
        }

        let name = read_file_string(&name_path)?;
        let normalized = name.to_lowercase();

        if KNOWN_GPU_NAMES.contains(&normalized.as_str()) || normalized.contains("gpu") {
            let real_path = fs::canonicalize(path.join("device"))?;
            gpus.push((name, real_path));
        }
    }

    Ok(gpus)
}

/// Read GPU 3D scaling status
pub fn read_gpu_3d_scaling(device_path: &Path) -> Result<Option<bool>> {
    let scaling_path = device_path.join("enable_3d_scaling");
    if !scaling_path.exists() {
        return Ok(None);
    }

    let value = read_file_u32(&scaling_path)?;
    Ok(Some(value == 1))
}

/// Set GPU 3D scaling
pub fn set_gpu_3d_scaling(name: &str, enabled: bool) -> Result<()> {
    let gpus = find_jetson_gpus()?;
    let gpu = gpus
        .iter()
        .find(|(n, _)| n == name)
        .ok_or_else(|| IronError::DeviceNotFound(format!("GPU '{}' not found", name)))?;

    let scaling_path = gpu.1.join("enable_3d_scaling");
    if !scaling_path.exists() {
        return Err(IronError::FeatureNotAvailable(
            "3D scaling not available for this GPU".to_string(),
        ));
    }

    write_file_string(&scaling_path, if enabled { "1" } else { "0" })?;
    Ok(())
}

/// Read GPU railgate status
pub fn read_gpu_railgate(device_path: &Path) -> Result<Option<bool>> {
    let railgate_path = device_path.join("railgate_enable");
    if !railgate_path.exists() {
        return Ok(None);
    }

    let value = read_file_u32(&railgate_path)?;
    Ok(Some(value == 1))
}

/// Set GPU railgate
pub fn set_gpu_railgate(name: &str, enabled: bool) -> Result<()> {
    let gpus = find_jetson_gpus()?;
    let gpu = gpus
        .iter()
        .find(|(n, _)| n == name)
        .ok_or_else(|| IronError::DeviceNotFound(format!("GPU '{}' not found", name)))?;

    let railgate_path = gpu.1.join("railgate_enable");
    if !railgate_path.exists() {
        return Err(IronError::FeatureNotAvailable(
            "Railgate not available for this GPU".to_string(),
        ));
    }

    write_file_string(&railgate_path, if enabled { "1" } else { "0" })?;
    Ok(())
}

/// Jetson PWM fan paths
const JETSON_FAN_PATHS: &[&str] = &[
    "/sys/class/hwmon",
    "/sys/devices/platform/pwm-fan",
    "/sys/devices/pwm-fan",
];

/// Find Jetson fan hwmon path
fn find_jetson_fan_hwmon() -> Option<PathBuf> {
    // Check hwmon devices
    if let Ok(entries) = fs::read_dir("/sys/class/hwmon") {
        for entry in entries.flatten() {
            let path = entry.path();

            // Check for pwm-fan name
            if let Ok(name) = fs::read_to_string(path.join("name")) {
                if name.trim() == "pwm-fan" || name.trim().contains("fan") {
                    return Some(path);
                }
            }
        }
    }

    // Check direct paths
    for base_path in JETSON_FAN_PATHS {
        let path = Path::new(base_path);
        if path.exists() {
            // For /sys/devices paths, look for hwmon subdir
            if let Ok(entries) = fs::read_dir(path.join("hwmon")) {
                if let Some(Ok(hwmon)) = entries.into_iter().next() {
                    return Some(hwmon.path());
                }
            }
            // Check for direct pwm file
            if path.join("pwm1").exists() || path.join("cur_pwm").exists() {
                return Some(path.to_path_buf());
            }
        }
    }

    None
}

/// Read current fan speed (0-255 PWM value)
pub fn read_fan_speed() -> Result<Option<u32>> {
    if let Some(hwmon) = find_jetson_fan_hwmon() {
        // Try pwm1 first (standard hwmon)
        if let Ok(value) = read_file_u32(hwmon.join("pwm1")) {
            return Ok(Some(value));
        }
        // Try cur_pwm (Jetson-specific)
        if let Ok(value) = read_file_u32(hwmon.join("cur_pwm")) {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

/// Read fan target speed
pub fn read_fan_target_speed() -> Result<Option<u32>> {
    if let Some(hwmon) = find_jetson_fan_hwmon() {
        if let Ok(value) = read_file_u32(hwmon.join("target_pwm")) {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

/// Set fan speed (0-255 PWM value or percentage 0-100)
pub fn set_fan_speed(_name: &str, speed: u32, _index: usize) -> Result<()> {
    let hwmon = find_jetson_fan_hwmon().ok_or_else(|| {
        IronError::FeatureNotAvailable("Fan control not available on this device".to_string())
    })?;

    // Convert percentage to PWM if needed (0-100 -> 0-255)
    let pwm_value = if speed <= 100 {
        (speed * 255 / 100).min(255)
    } else {
        speed.min(255)
    };

    // Try target_pwm first (Jetson-specific)
    let target_path = hwmon.join("target_pwm");
    if target_path.exists() {
        write_file_string(&target_path, &pwm_value.to_string())?;
        return Ok(());
    }

    // Try pwm1 (standard hwmon)
    let pwm_path = hwmon.join("pwm1");
    if pwm_path.exists() {
        // May need to disable pwm_enable first (set manual mode)
        let enable_path = hwmon.join("pwm1_enable");
        if enable_path.exists() {
            // 1 = manual mode
            let _ = write_file_string(&enable_path, "1");
        }
        write_file_string(&pwm_path, &pwm_value.to_string())?;
        return Ok(());
    }

    Err(IronError::FeatureNotAvailable(
        "No writable fan control interface found".to_string(),
    ))
}

/// Jetson fan profiles
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FanProfile {
    /// Quiet mode - low fan speed, may allow higher temperatures
    Quiet,
    /// Cool mode - higher fan speed for lower temperatures
    Cool,
    /// Default automatic mode
    Auto,
    /// Manual control - user sets speed directly
    Manual,
}

impl FanProfile {
    fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "quiet" => Some(FanProfile::Quiet),
            "cool" => Some(FanProfile::Cool),
            "auto" | "automatic" | "default" => Some(FanProfile::Auto),
            "manual" => Some(FanProfile::Manual),
            _ => None,
        }
    }

    fn to_pwm_enable(self) -> u32 {
        match self {
            FanProfile::Manual => 1, // Manual control
            FanProfile::Auto | FanProfile::Quiet | FanProfile::Cool => 2, // Automatic
        }
    }
}

/// Set fan profile
pub fn set_fan_profile(_name: &str, profile: &str) -> Result<()> {
    let profile = FanProfile::from_str(profile).ok_or_else(|| {
        IronError::Parse(format!(
            "Unknown fan profile '{}'. Valid options: quiet, cool, auto, manual",
            profile
        ))
    })?;

    let hwmon = find_jetson_fan_hwmon().ok_or_else(|| {
        IronError::FeatureNotAvailable("Fan control not available on this device".to_string())
    })?;

    // Set pwm_enable for mode
    let enable_path = hwmon.join("pwm1_enable");
    if enable_path.exists() {
        write_file_string(&enable_path, &profile.to_pwm_enable().to_string())?;
    }

    // Set speed based on profile
    match profile {
        FanProfile::Quiet => {
            // Quiet: 30% fan speed
            let target_path = hwmon.join("target_pwm");
            if target_path.exists() {
                write_file_string(&target_path, "76")?; // 30% of 255
            }
        }
        FanProfile::Cool => {
            // Cool: 100% fan speed
            let target_path = hwmon.join("target_pwm");
            if target_path.exists() {
                write_file_string(&target_path, "255")?;
            }
        }
        FanProfile::Auto | FanProfile::Manual => {
            // Auto mode or manual - don't change speed
        }
    }

    Ok(())
}

/// Get current fan profile
pub fn get_fan_profile() -> Result<Option<String>> {
    let hwmon = find_jetson_fan_hwmon();

    if let Some(hwmon) = hwmon {
        let enable_path = hwmon.join("pwm1_enable");
        if let Ok(value) = read_file_u32(&enable_path) {
            let profile = match value {
                0 => "off",
                1 => "manual",
                2 => "auto",
                _ => "unknown",
            };
            return Ok(Some(profile.to_string()));
        }
    }

    Ok(None)
}

/// Read L4T version from Jetson
pub fn read_l4t_version() -> Option<String> {
    // Try reading from /etc/nv_tegra_release
    if let Ok(content) = fs::read_to_string("/etc/nv_tegra_release") {
        // Parse version from content like: "# R32 (release), REVISION: 7.1"
        for line in content.lines() {
            if line.contains("R") && line.contains("REVISION") {
                return Some(line.to_string());
            }
        }
    }
    None
}

/// Get Jetson model from device tree
pub fn read_jetson_model() -> Option<String> {
    read_file_string("/sys/firmware/devicetree/base/model").ok()
}

/// Get Jetson serial number
pub fn read_serial_number() -> Option<String> {
    read_file_string("/sys/firmware/devicetree/base/serial-number").ok()
}
