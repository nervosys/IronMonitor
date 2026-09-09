//! NVPModel - Power mode management for Jetson
//!
//! NVPModel controls voltage regulators and power tree to optimize power efficiency.
//! It supports various power budgets with different CPU/GPU configurations.

use super::verify_sudo_available;
use crate::error::{IronError, Result};
use std::process::Command;

/// Power mode information
#[derive(Debug, Clone)]
pub struct PowerMode {
    /// Mode ID
    pub id: u32,
    /// Mode name
    pub name: String,
    /// Whether this is the default mode
    pub is_default: bool,
}

/// NVPModel status
#[derive(Debug, Clone)]
pub struct NVPModelStatus {
    /// Current power mode
    pub current: PowerMode,
    /// All available power modes
    pub modes: Vec<PowerMode>,
    /// Default mode
    pub default: PowerMode,
}

/// Check if nvpmodel is available
pub fn is_available() -> bool {
    Command::new("which")
        .arg("nvpmodel")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Get current nvpmodel status
pub fn query() -> Result<PowerMode> {
    let output = Command::new("nvpmodel")
        .arg("-q")
        .output()
        .map_err(IronError::Io)?;

    if !output.status.success() {
        return Err(IronError::CommandFailed(
            "nvpmodel query failed".to_string(),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut name = String::new();
    let mut id = 0u32;

    for (idx, line) in stdout.lines().enumerate() {
        if line.contains("NV Power Mode:") {
            name = line.split(':').nth(1).unwrap_or("").trim().to_string();

            // Next line should have the ID
            if let Some(next_line) = stdout.lines().nth(idx + 1) {
                id = next_line.trim().parse().unwrap_or(0);
            }
            break;
        }
    }

    Ok(PowerMode {
        id,
        name,
        is_default: false,
    })
}

/// List all available power modes
pub fn list_modes() -> Result<NVPModelStatus> {
    let output = Command::new("nvpmodel")
        .arg("-p")
        .arg("--verbose")
        .output()
        .map_err(IronError::Io)?;

    if !output.status.success() {
        return Err(IronError::CommandFailed("nvpmodel list failed".to_string()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut modes = Vec::new();
    let mut default = PowerMode {
        id: 0,
        name: String::new(),
        is_default: true,
    };

    for line in stdout.lines() {
        // Parse default: PM_CONFIG: DEFAULT=MAXN(0)
        if line.contains("PM_CONFIG: DEFAULT=") {
            if let Some(default_str) = line.split("DEFAULT=").nth(1) {
                if let Some(name_part) = default_str.split('(').next() {
                    default.name = name_part.to_string();
                }
                if let Some(id_part) = default_str.split('(').nth(1) {
                    if let Some(id_str) = id_part.split(')').next() {
                        default.id = id_str.parse().unwrap_or(0);
                    }
                }
            }
        }

        // Parse modes: POWER_MODEL: ID=0 NAME=MAXN
        if line.contains("POWER_MODEL: ID=") {
            let mut mode_id = 0u32;
            let mut mode_name = String::new();

            if let Some(id_str) = line.split("ID=").nth(1) {
                if let Some(id_part) = id_str.split_whitespace().next() {
                    mode_id = id_part.parse().unwrap_or(0);
                }
            }

            if let Some(name_str) = line.split("NAME=").nth(1) {
                mode_name = name_str.trim().to_string();
            }

            modes.push(PowerMode {
                id: mode_id,
                name: mode_name,
                is_default: mode_id == default.id,
            });
        }
    }

    let current = query()?;

    Ok(NVPModelStatus {
        current,
        modes,
        default,
    })
}

/// Set power mode by ID
pub fn set_mode(mode_id: u32, force: bool) -> Result<()> {
    // Security: Verify sudo is available
    verify_sudo_available()?;

    let mut cmd = Command::new("sudo");
    cmd.arg("nvpmodel").arg("-m").arg(mode_id.to_string());

    let output = if force {
        cmd.arg("-f").output()
    } else {
        cmd.output()
    }
    .map_err(IronError::Io)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(IronError::CommandFailed(format!(
            "Failed to set power mode: {}",
            stderr
        )));
    }

    // Check for errors in output
    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.contains("NVPM ERROR") {
        return Err(IronError::CommandFailed(
            "nvpmodel reported an error".to_string(),
        ));
    }

    Ok(())
}

/// Set power mode by name
pub fn set_mode_by_name(mode_name: &str, force: bool) -> Result<()> {
    let status = list_modes()?;

    let mode = status
        .modes
        .iter()
        .find(|m| m.name.eq_ignore_ascii_case(mode_name))
        .ok_or_else(|| IronError::InvalidValue(format!("Power mode '{}' not found", mode_name)))?;

    set_mode(mode.id, force)
}
