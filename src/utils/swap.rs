//! Swap management utility for Jetson
//!
//! Create, enable, disable, and manage swap files.

use super::verify_sudo_available;
use crate::error::{IronError, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Maximum swap file size in GB to prevent disk exhaustion
const MAX_SWAP_SIZE_GB: u32 = 64;

/// Allowed directories for swap file creation (security whitelist)
const ALLOWED_SWAP_DIRS: &[&str] = &["/swapfile", "/var/swap", "/mnt/swap", "/tmp", "/home"];

/// Swap file information
#[derive(Debug, Clone)]
pub struct SwapInfo {
    /// Swap file path
    pub path: String,
    /// Type (file or partition)
    pub swap_type: String,
    /// Priority
    pub priority: i32,
    /// Total size in KB
    pub size_kb: u64,
    /// Used size in KB
    pub used_kb: u64,
}

/// Read current swap status
pub fn status() -> Result<Vec<SwapInfo>> {
    let output = Command::new("swapon")
        .arg("--show")
        .arg("--noheadings")
        .arg("--raw")
        .output()
        .map_err(IronError::Io)?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut swaps = Vec::new();

    for line in stdout.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 5 {
            swaps.push(SwapInfo {
                path: parts[0].to_string(),
                swap_type: parts[1].to_string(),
                size_kb: parse_size(parts[2]),
                used_kb: parse_size(parts[3]),
                priority: parts[4].parse().unwrap_or(0),
            });
        }
    }

    Ok(swaps)
}

fn parse_size(size_str: &str) -> u64 {
    // Parse sizes like "1.5G", "512M", "2048K"
    let size_str = size_str.trim();
    if size_str.is_empty() {
        return 0;
    }

    // Security: Check for last character safely without unwrap
    let last_char = match size_str.chars().last() {
        Some(c) => c,
        None => return 0,
    };

    let (num_part, unit) = if last_char.is_alphabetic() {
        let last_idx = size_str.len() - last_char.len_utf8();
        (&size_str[..last_idx], last_char)
    } else {
        (size_str, 'K')
    };

    let num: f64 = num_part.parse().unwrap_or(0.0);

    match unit {
        'K' | 'k' => num as u64,
        'M' | 'm' => (num * 1024.0) as u64,
        'G' | 'g' => (num * 1024.0 * 1024.0) as u64,
        _ => num as u64,
    }
}

/// Validate and sanitize swap file path for security
fn validate_swap_path(path: &Path) -> Result<PathBuf> {
    // Convert to absolute path
    let abs_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().map_err(IronError::Io)?.join(path)
    };

    // Get parent directory for validation
    let parent = abs_path
        .parent()
        .ok_or_else(|| IronError::InvalidValue("Invalid path: no parent directory".into()))?;

    // Check if parent exists and canonicalize to resolve symlinks
    let canonical_parent = if parent.exists() {
        parent.canonicalize().map_err(IronError::Io)?
    } else {
        return Err(IronError::InvalidValue(format!(
            "Parent directory does not exist: {}",
            parent.display()
        )));
    };

    // Security check: ensure path is in allowed directories
    let path_str = canonical_parent.to_string_lossy();
    let is_allowed = ALLOWED_SWAP_DIRS
        .iter()
        .any(|allowed_dir| path_str.starts_with(allowed_dir));

    if !is_allowed {
        return Err(IronError::PermissionDenied(format!(
            "Swap files only allowed in approved directories: {:?}. Got: {}",
            ALLOWED_SWAP_DIRS,
            canonical_parent.display()
        )));
    }

    // Reconstruct full path with canonical parent
    let filename = abs_path
        .file_name()
        .ok_or_else(|| IronError::InvalidValue("Invalid filename".into()))?;

    let validated_path = canonical_parent.join(filename);

    // Security check: ensure the final path doesn't exist as a symlink
    if validated_path.exists() && validated_path.is_symlink() {
        return Err(IronError::InvalidValue(
            "Path is a symlink, which is not allowed for security reasons".into(),
        ));
    }

    Ok(validated_path)
}

/// Create a swap file
pub fn create(path: &Path, size_gb: u32, enable_on_boot: bool) -> Result<()> {
    // Security: Verify sudo is available before proceeding
    verify_sudo_available()?;

    // Security: Validate size limit to prevent disk exhaustion
    if size_gb == 0 {
        return Err(IronError::InvalidValue(
            "Swap size must be greater than 0".into(),
        ));
    }

    if size_gb > MAX_SWAP_SIZE_GB {
        return Err(IronError::InvalidValue(format!(
            "Swap size exceeds maximum allowed: {} GB (max: {} GB)",
            size_gb, MAX_SWAP_SIZE_GB
        )));
    }

    // Security: Validate and sanitize path
    let validated_path = validate_swap_path(path)?;

    // Check if already exists
    if validated_path.exists() {
        return Err(IronError::InvalidValue(format!(
            "Swap file already exists: {}",
            validated_path.display()
        )));
    }

    println!(
        "Creating swap file: {} [{} GB]",
        validated_path.display(),
        size_gb
    );

    // Calculate block size (1M blocks)
    let block_count = size_gb * 1024;

    // Create swap file with dd
    let output = Command::new("sudo")
        .arg("dd")
        .arg("if=/dev/zero")
        .arg(format!("of={}", validated_path.display()))
        .arg("bs=1M")
        .arg(format!("count={}", block_count))
        .arg("status=progress")
        .output()
        .map_err(IronError::Io)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(IronError::CommandFailed(format!(
            "Failed to create swap file: {}",
            stderr
        )));
    }

    // Set permissions
    Command::new("sudo")
        .arg("chmod")
        .arg("600")
        .arg(&validated_path)
        .output()
        .map_err(IronError::Io)?;

    // Make swap
    let output = Command::new("sudo")
        .arg("mkswap")
        .arg(&validated_path)
        .output()
        .map_err(IronError::Io)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(IronError::CommandFailed(format!(
            "Failed to make swap: {}",
            stderr
        )));
    }

    // Enable swap
    enable(&validated_path)?;

    // Add to fstab if requested
    if enable_on_boot {
        add_to_fstab(&validated_path)?;
    }

    println!("Swap file created and enabled successfully");
    Ok(())
}

/// Enable swap file
pub fn enable(path: &Path) -> Result<()> {
    // Security: Verify sudo is available
    verify_sudo_available()?;

    let output = Command::new("sudo")
        .arg("swapon")
        .arg(path)
        .output()
        .map_err(IronError::Io)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(IronError::CommandFailed(format!(
            "Failed to enable swap: {}",
            stderr
        )));
    }

    Ok(())
}

/// Disable swap file
pub fn disable(path: &Path) -> Result<()> {
    // Security: Verify sudo is available
    verify_sudo_available()?;

    let output = Command::new("sudo")
        .arg("swapoff")
        .arg(path)
        .output()
        .map_err(IronError::Io)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(IronError::CommandFailed(format!(
            "Failed to disable swap: {}",
            stderr
        )));
    }

    Ok(())
}

/// Add swap to /etc/fstab for boot persistence
fn add_to_fstab(path: &Path) -> Result<()> {
    use std::io::Write;

    // Canonicalize path to prevent injection
    let canonical_path = path.canonicalize().map_err(IronError::Io)?;

    let fstab_entry = format!("{} none swap sw 0 0\n", canonical_path.display());

    // Check if already in fstab
    if let Ok(fstab) = fs::read_to_string("/etc/fstab") {
        if fstab.contains(&canonical_path.display().to_string()) {
            println!("Swap already in /etc/fstab");
            return Ok(());
        }
    }

    // Use tee to append (more secure than shell, no command injection possible)
    let mut child = Command::new("sudo")
        .arg("tee")
        .arg("-a")
        .arg("/etc/fstab")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .spawn()
        .map_err(IronError::Io)?;

    // Write to stdin
    if let Some(ref mut stdin) = child.stdin {
        stdin
            .write_all(fstab_entry.as_bytes())
            .map_err(IronError::Io)?;
    }

    let output = child.wait_with_output().map_err(IronError::Io)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(IronError::CommandFailed(format!(
            "Failed to add to fstab: {}",
            stderr
        )));
    }

    println!("Added swap to /etc/fstab for automatic enable on boot");
    Ok(())
}

/// Remove swap file
pub fn remove(path: &Path) -> Result<()> {
    // Security: Verify sudo is available
    verify_sudo_available()?;

    // Disable first
    if let Err(e) = disable(path) {
        eprintln!("Warning: Could not disable swap: {}", e);
    }

    // Remove file
    let output = Command::new("sudo")
        .arg("rm")
        .arg(path)
        .output()
        .map_err(IronError::Io)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(IronError::CommandFailed(format!(
            "Failed to remove swap file: {}",
            stderr
        )));
    }

    println!("Swap file removed: {}", path.display());
    Ok(())
}
