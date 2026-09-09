//! Jetson Clocks - Performance maximization utility
//!
//! This module provides functionality to maximize Jetson performance by setting
//! all frequencies (CPU, GPU, EMC, engines) to their maximum values.

use super::verify_sudo_available;
use crate::error::{IronError, Result};
use std::fs;
use std::path::Path;
use std::process::Command;

const JETSON_CLOCKS_PATHS: &[&str] = &["/usr/bin/jetson_clocks", "/home/nvidia/jetson_clocks.sh"];

/// Jetson Clocks status
#[derive(Debug, Clone)]
pub struct JetsonClocksStatus {
    /// Whether jetson_clocks is active
    pub active: bool,
    /// Engines configured
    pub engines: Vec<String>,
}

/// Check if jetson_clocks is available
pub fn is_available() -> bool {
    JETSON_CLOCKS_PATHS.iter().any(|p| Path::new(p).exists())
}

/// Find jetson_clocks executable
fn find_jetson_clocks() -> Result<String> {
    JETSON_CLOCKS_PATHS
        .iter()
        .find(|p| Path::new(p).exists())
        .map(|s| s.to_string())
        .ok_or_else(|| IronError::DeviceNotFound("jetson_clocks not found".to_string()))
}

/// Enable jetson_clocks (maximize performance)
pub fn enable() -> Result<()> {
    // Security: Verify sudo is available
    verify_sudo_available()?;

    let jetson_clocks = find_jetson_clocks()?;

    let output = Command::new("sudo")
        .arg(&jetson_clocks)
        .output()
        .map_err(IronError::Io)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(IronError::CommandFailed(format!(
            "Failed to enable jetson_clocks: {}",
            stderr
        )));
    }

    Ok(())
}

/// Disable jetson_clocks (restore original settings)
pub fn disable() -> Result<()> {
    // Security: Verify sudo is available
    verify_sudo_available()?;

    let jetson_clocks = find_jetson_clocks()?;

    let output = Command::new("sudo")
        .arg(&jetson_clocks)
        .arg("--restore")
        .output()
        .map_err(IronError::Io)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(IronError::CommandFailed(format!(
            "Failed to disable jetson_clocks: {}",
            stderr
        )));
    }

    Ok(())
}

/// Show jetson_clocks status
pub fn show() -> Result<JetsonClocksStatus> {
    let jetson_clocks = find_jetson_clocks()?;

    let output = Command::new(&jetson_clocks)
        .arg("--show")
        .output()
        .map_err(IronError::Io)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(IronError::CommandFailed(format!(
            "Failed to get jetson_clocks status: {}",
            stderr
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let engines: Vec<String> = stdout
        .lines()
        .filter(|line| line.contains("MaxFreq="))
        .map(|line| {
            line.split_whitespace()
                .next()
                .unwrap_or("unknown")
                .to_string()
        })
        .collect();

    // Check if active by examining current frequencies
    let active = check_if_active()?;

    Ok(JetsonClocksStatus { active, engines })
}

/// Check if jetson_clocks is currently active
fn check_if_active() -> Result<bool> {
    // Check if CPU frequencies are maxed
    if let Ok(cpu_online) = fs::read_to_string("/sys/devices/system/cpu/online") {
        for cpu_range in cpu_online.trim().split(',') {
            if let Some((start, end)) = parse_cpu_range(cpu_range) {
                for cpu in start..=end {
                    let min_path = format!(
                        "/sys/devices/system/cpu/cpu{}/cpufreq/scaling_min_freq",
                        cpu
                    );
                    let max_path = format!(
                        "/sys/devices/system/cpu/cpu{}/cpufreq/scaling_max_freq",
                        cpu
                    );

                    if let (Ok(min), Ok(max)) =
                        (fs::read_to_string(&min_path), fs::read_to_string(&max_path))
                    {
                        let min_freq: u32 = min.trim().parse().unwrap_or(0);
                        let max_freq: u32 = max.trim().parse().unwrap_or(0);

                        if min_freq != max_freq {
                            return Ok(false);
                        }
                    }
                }
            }
        }
    }

    Ok(true)
}

fn parse_cpu_range(range: &str) -> Option<(u32, u32)> {
    if range.contains('-') {
        let parts: Vec<&str> = range.split('-').collect();
        if parts.len() == 2 {
            let start = parts[0].parse().ok()?;
            let end = parts[1].parse().ok()?;
            return Some((start, end));
        }
    } else {
        let cpu = range.parse().ok()?;
        return Some((cpu, cpu));
    }
    None
}

/// Store current configuration
pub fn store() -> Result<()> {
    // Security: Verify sudo is available
    verify_sudo_available()?;

    let jetson_clocks = find_jetson_clocks()?;

    let output = Command::new("sudo")
        .arg(&jetson_clocks)
        .arg("--store")
        .output()
        .map_err(IronError::Io)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(IronError::CommandFailed(format!(
            "Failed to store jetson_clocks config: {}",
            stderr
        )));
    }

    Ok(())
}
