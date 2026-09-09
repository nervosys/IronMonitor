//! Error types for IronMonitor

use std::io;
use thiserror::Error;

/// Result type alias for IronMonitor operations (legacy compatibility)
pub type Result<T> = std::result::Result<T, IronError>;

/// Legacy error type for backward compatibility
#[derive(Error, Debug)]
pub enum IronError {
    /// I/O error
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    /// NVML error
    #[cfg(feature = "nvidia")]
    #[error("NVML error: {0}")]
    Nvml(#[from] nvml_wrapper::error::NvmlError),

    /// Parse error
    #[error("Parse error: {0}")]
    Parse(String),

    /// Device not found
    #[error("Device not found: {0}")]
    DeviceNotFound(String),

    /// Permission denied
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    /// Unsupported platform
    #[error("Unsupported platform: {0}")]
    UnsupportedPlatform(String),

    /// Feature not available
    #[error("Feature not available: {0}")]
    FeatureNotAvailable(String),

    /// Invalid value
    #[error("Invalid value: {0}")]
    InvalidValue(String),

    /// Command failed
    #[error("Command failed: {0}")]
    CommandFailed(String),

    /// System error
    #[error("System error: {0}")]
    System(String),

    /// Initialization error
    #[error("Initialization error: {0}")]
    InitializationError(String),

    /// Network error (for remote backends)
    #[error("Network error: {0}")]
    Network(String),

    /// Agent/AI backend error
    #[error("Agent error: {0}")]
    Agent(String),

    /// Not implemented
    #[error("Not implemented: {0}")]
    NotImplemented(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    Configuration(String),

    /// GPU-specific error
    #[error("GPU error: {0}")]
    GpuError(String),

    /// Invalid input
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    /// Resource not found
    #[error("Not found: {0}")]
    NotFound(String),

    /// Process-related error
    #[error("Process error: {0}")]
    ProcessError(String),

    /// CPU-related error
    #[error("CPU error: {0}")]
    CpuError(String),

    /// Memory-related error
    #[error("Memory error: {0}")]
    MemoryError(String),

    /// Disk-related error
    #[error("Disk error: {0}")]
    DiskError(String),

    /// Hardware-related error
    #[error("Hardware error: {0}")]
    HardwareError(String),

    /// Invalid argument
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    /// JSON serialization error
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    /// Other error
    #[error("{0}")]
    Other(String),
}

/// Main error type for IronMonitor
#[derive(Error, Debug)]
pub enum Error {
    /// I/O error
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    /// NVML error (NVIDIA GPUs)
    #[cfg(feature = "nvidia")]
    #[error("NVML error: {0}")]
    Nvml(#[from] nvml_wrapper::error::NvmlError),

    /// GPU-specific error
    #[error("GPU error: {0}")]
    GpuError(String),

    /// Process-related error
    #[error("Process error: {0}")]
    ProcessError(String),

    /// Feature not supported
    #[error("Not supported: {0}")]
    NotSupported(String),

    /// Permission denied
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    /// Device not found
    #[error("Device not found: {0}")]
    DeviceNotFound(String),

    /// Invalid parameter
    #[error("Invalid parameter: {0}")]
    InvalidParameter(String),

    /// Parse error
    #[error("Parse error: {0}")]
    ParseError(String),

    /// Command execution failed
    #[error("Command execution failed: {0}")]
    CommandExecutionFailed(String),

    /// Feature not supported
    #[error("Unsupported: {0}")]
    Unsupported(String),

    /// System error
    #[error("System error: {0}")]
    SystemError(String),

    /// Nix error (Unix)
    #[cfg(unix)]
    #[error("Nix error: {0}")]
    Nix(#[from] nix::Error),

    /// Legacy error for backward compatibility
    #[error("Legacy error: {0}")]
    Legacy(#[from] IronError),

    /// Other error
    #[error("{0}")]
    Other(String),
}

impl From<Error> for IronError {
    fn from(err: Error) -> Self {
        match err {
            Error::Io(e) => IronError::Io(e),
            #[cfg(feature = "nvidia")]
            Error::Nvml(e) => IronError::Nvml(e),
            Error::GpuError(s)
            | Error::ProcessError(s)
            | Error::SystemError(s)
            | Error::CommandExecutionFailed(s)
            | Error::Other(s) => IronError::Other(s),
            Error::NotSupported(s) | Error::Unsupported(s) => IronError::FeatureNotAvailable(s),
            Error::PermissionDenied(s) => IronError::PermissionDenied(s),
            Error::DeviceNotFound(s) => IronError::DeviceNotFound(s),
            Error::InvalidParameter(s) | Error::ParseError(s) => IronError::Parse(s),
            #[cfg(unix)]
            Error::Nix(e) => IronError::System(e.to_string()),
            Error::Legacy(e) => e,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // === IronError tests ===

    #[test]
    fn test_iron_error_display_parse() {
        let err = IronError::Parse("bad value".to_string());
        assert_eq!(err.to_string(), "Parse error: bad value");
    }

    #[test]
    fn test_iron_error_display_device_not_found() {
        let err = IronError::DeviceNotFound("GPU 0".to_string());
        assert_eq!(err.to_string(), "Device not found: GPU 0");
    }

    #[test]
    fn test_iron_error_display_permission_denied() {
        let err = IronError::PermissionDenied("need root".to_string());
        assert_eq!(err.to_string(), "Permission denied: need root");
    }

    #[test]
    fn test_iron_error_display_unsupported_platform() {
        let err = IronError::UnsupportedPlatform("FreeBSD".to_string());
        assert_eq!(err.to_string(), "Unsupported platform: FreeBSD");
    }

    #[test]
    fn test_iron_error_display_not_implemented() {
        let err = IronError::NotImplemented("macOS fan".to_string());
        assert_eq!(err.to_string(), "Not implemented: macOS fan");
    }

    #[test]
    fn test_iron_error_from_io() {
        let io_err = io::Error::new(io::ErrorKind::NotFound, "file missing");
        let err: IronError = io_err.into();
        assert!(err.to_string().contains("file missing"));
    }

    #[test]
    fn test_iron_error_from_json() {
        let json_str = "{ invalid json }}}";
        let json_err = serde_json::from_str::<serde_json::Value>(json_str).unwrap_err();
        let err: IronError = json_err.into();
        assert!(err.to_string().contains("JSON error"));
    }

    #[test]
    fn test_iron_error_gpu_error() {
        let err = IronError::GpuError("NVML failure".to_string());
        assert_eq!(err.to_string(), "GPU error: NVML failure");
    }

    #[test]
    fn test_iron_error_other() {
        let err = IronError::Other("misc error".to_string());
        assert_eq!(err.to_string(), "misc error");
    }

    // === Error (unified GPU error) tests ===

    #[test]
    fn test_error_not_supported() {
        let err = Error::NotSupported("MIG".to_string());
        assert!(err.to_string().contains("Not supported"));
    }

    #[test]
    fn test_error_gpu_error_display() {
        let err = Error::GpuError("No devices".to_string());
        assert!(err.to_string().contains("No devices"));
    }

    #[test]
    fn test_error_conversion_to_iron_error() {
        let err = Error::GpuError("test failure".to_string());
        let ironmon_err: IronError = err.into();
        assert!(ironmon_err.to_string().contains("test failure"));
    }

    #[test]
    fn test_error_not_supported_conv() {
        let err = Error::NotSupported("feature X".to_string());
        let ironmon_err: IronError = err.into();
        match ironmon_err {
            IronError::FeatureNotAvailable(s) => assert_eq!(s, "feature X"),
            _ => panic!("Expected FeatureNotAvailable"),
        }
    }

    #[test]
    fn test_error_permission_denied_conv() {
        let err = Error::PermissionDenied("need admin".to_string());
        let ironmon_err: IronError = err.into();
        match ironmon_err {
            IronError::PermissionDenied(s) => assert_eq!(s, "need admin"),
            _ => panic!("Expected PermissionDenied"),
        }
    }

    #[test]
    fn test_error_device_not_found_conv() {
        let err = Error::DeviceNotFound("GPU 1".to_string());
        let ironmon_err: IronError = err.into();
        match ironmon_err {
            IronError::DeviceNotFound(s) => assert_eq!(s, "GPU 1"),
            _ => panic!("Expected DeviceNotFound"),
        }
    }
}
