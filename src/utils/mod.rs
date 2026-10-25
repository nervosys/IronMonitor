//! Utility modules for advanced Jetson management

pub mod clocks;
pub mod power_mode;
pub mod swap;

mod security;
pub(crate) use security::{sudo_command, sudo_failure, verify_sudo_available};
