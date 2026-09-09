//! Common platform utilities

use crate::error::{IronError, Result};
use std::fs;
use std::path::Path;

/// Read file contents as string, trimming whitespace
pub fn read_file_string<P: AsRef<Path>>(path: P) -> Result<String> {
    Ok(fs::read_to_string(path)?.trim().to_string())
}

/// Read file and parse as integer
pub fn read_file_u64<P: AsRef<Path>>(path: P) -> Result<u64> {
    let content = read_file_string(path)?;
    content
        .parse()
        .map_err(|e| IronError::Parse(format!("Failed to parse u64: {}", e)))
}

/// Read file and parse as u32
pub fn read_file_u32<P: AsRef<Path>>(path: P) -> Result<u32> {
    let content = read_file_string(path)?;
    content
        .parse()
        .map_err(|e| IronError::Parse(format!("Failed to parse u32: {}", e)))
}

/// Read file and parse as f32
pub fn read_file_f32<P: AsRef<Path>>(path: P) -> Result<f32> {
    let content = read_file_string(path)?;
    content
        .parse()
        .map_err(|e| IronError::Parse(format!("Failed to parse f32: {}", e)))
}

/// Check if file/directory exists
pub fn path_exists<P: AsRef<Path>>(path: P) -> bool {
    path.as_ref().exists()
}

/// Check if we have read access to a file
pub fn has_read_access<P: AsRef<Path>>(path: P) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = fs::metadata(path) {
            let permissions = metadata.permissions();
            // Check if readable by owner, group, or other
            permissions.mode() & 0o444 != 0
        } else {
            false
        }
    }

    #[cfg(not(unix))]
    {
        // On Windows, just try to read
        path.as_ref().exists()
    }
}

/// Write string to file
pub fn write_file_string<P: AsRef<Path>>(path: P, content: &str) -> Result<()> {
    fs::write(path, content)?;
    Ok(())
}
