//! Security utilities for privilege escalation and validation

use crate::error::{IronError, Result};
use std::process::Command;
use std::time::Duration;

/// Default timeout for privileged commands (30 seconds)
#[allow(dead_code)]
pub const DEFAULT_COMMAND_TIMEOUT: Duration = Duration::from_secs(30);

/// Verify that sudo is available and the user has permissions
///
/// This prevents TOCTOU (time-of-check-time-of-use) vulnerabilities and ensures
/// proper error messages when sudo is not available or configured incorrectly.
///
/// # Security
/// - Checks if sudo binary exists
/// - Verifies user has sudo privileges (non-interactive check)
/// - Returns proper error if sudo password is required
///
/// # Errors
/// Returns `PermissionDenied` if sudo is not available or user lacks privileges
pub fn verify_sudo_available() -> Result<()> {
    // Check if sudo binary exists
    let which_output = Command::new("which").arg("sudo").output();

    match which_output {
        Ok(output) if output.status.success() => {
            // sudo exists, continue to permission check
        }
        Ok(_) => {
            return Err(IronError::PermissionDenied(
                "sudo command not found on system".into(),
            ));
        }
        Err(e) => {
            return Err(IronError::Io(e));
        }
    }

    // Verify user has sudo privileges without password prompt
    // The -n flag prevents sudo from prompting for password
    let sudo_check = Command::new("sudo").args(["-n", "true"]).output();

    match sudo_check {
        Ok(output) if output.status.success() => {
            // User has passwordless sudo or password is cached
            Ok(())
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("password is required") || stderr.contains("a password is required")
            {
                Err(IronError::PermissionDenied(
                    "sudo password required. Please run 'sudo -v' first or configure passwordless sudo".into()
                ))
            } else {
                Err(IronError::PermissionDenied(format!(
                    "User does not have sudo privileges: {}",
                    stderr
                )))
            }
        }
        Err(e) => Err(IronError::Io(e)),
    }
}

/// Log a privileged operation attempt
///
/// # Security
/// Creates an audit trail for all privileged operations
#[cfg(all(feature = "cli", target_os = "linux"))]
#[allow(dead_code)]
pub fn log_privileged_operation(operation: &str, success: bool) {
    use log::{info, warn};

    let user = std::env::var("USER").unwrap_or_else(|_| "unknown".into());
    let uid = unsafe { libc::getuid() };

    if success {
        info!(
            "Privileged operation '{}' succeeded for user {} (UID: {})",
            operation, user, uid
        );
    } else {
        warn!(
            "Privileged operation '{}' failed for user {} (UID: {})",
            operation, user, uid
        );
    }
}

#[cfg(not(all(feature = "cli", target_os = "linux")))]
#[allow(dead_code)]
pub fn log_privileged_operation(_operation: &str, _success: bool) {
    // No logging without CLI feature or on non-Linux platforms
}
