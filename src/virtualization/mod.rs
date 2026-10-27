// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2026 nervosys

//! Virtualization and container detection and monitoring
//!
//! Detects hypervisors, container runtimes, cloud providers,
//! and provides resource visibility for virtualized environments.

pub mod containers;
pub mod detect;
pub mod guest;

pub use containers::{
    ContainerEngine, ContainerInfo, ContainerResources, K8sPodInfo, OrchestratorInfo,
};
pub use detect::{CpuVirtCapability, Hypervisor, HypervisorInfo, VirtPlatform};
pub use guest::{BalloonInfo, GuestAgent, GuestResources, VirtCpuTopology, VirtDisk, VirtNic};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum VirtError {
    #[error("Detection failed: {0}")]
    DetectionFailed(String),
    #[error("Not supported on this platform: {0}")]
    NotSupported(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Complete virtualization snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VirtSnapshot {
    pub platform: VirtPlatform,
    pub hypervisor: Option<HypervisorInfo>,
    pub container: Option<ContainerInfo>,
    pub guest_resources: Option<GuestResources>,
    pub cpu_capabilities: Option<CpuVirtCapability>,
    pub orchestrator: Option<OrchestratorInfo>,
    pub timestamp: u64,
}

/// Main virtualization monitor
pub struct VirtMonitor {
    _platform: VirtPlatform,
}

impl VirtMonitor {
    pub fn new() -> Result<Self, VirtError> {
        let platform = detect::detect_platform();
        Ok(Self {
            _platform: platform,
        })
    }

    /// Detect hypervisor if running in a VM
    pub fn hypervisor(&self) -> Option<HypervisorInfo> {
        detect::detect_hypervisor()
    }

    /// Check if running inside a virtual machine
    /// Whether this is running inside a virtual machine.
    ///
    /// A Hyper-V *root* partition is not a VM, however loudly CPUID says
    /// "Microsoft Hv". Windows 11 turns on virtualization-based security by
    /// default, so an ordinary desktop carries the hypervisor signature; before
    /// 3.10.0 this method returned true on bare metal for that reason.
    ///
    /// `None` when no signal could be read to decide it.
    pub fn is_virtual_machine(&self) -> Option<bool> {
        detect::hypervisor_indicates_vm()
    }

    /// Check if running inside a container.
    ///
    /// `None` on platforms where container detection is not implemented (only
    /// Linux is). Reporting `false` there would claim a container check that
    /// never ran.
    pub fn is_container(&self) -> Option<bool> {
        #[cfg(target_os = "linux")]
        {
            Some(containers::detect_container().is_some())
        }
        #[cfg(not(target_os = "linux"))]
        {
            None
        }
    }

    /// Check if running on bare metal. Known only when both the VM and the
    /// container questions were answered.
    pub fn is_bare_metal(&self) -> Option<bool> {
        match (self.is_virtual_machine(), self.is_container()) {
            (Some(true), _) | (_, Some(true)) => Some(false),
            (Some(false), Some(false)) => Some(true),
            _ => None,
        }
    }

    /// Get guest resource info
    pub fn guest_resources(&self) -> Option<GuestResources> {
        guest::detect_guest_resources()
    }

    /// Get container info
    pub fn container_info(&self) -> Option<ContainerInfo> {
        containers::detect_container()
    }

    /// Get CPU virtualization capabilities
    pub fn cpu_capabilities(&self) -> Option<CpuVirtCapability> {
        detect::detect_cpu_virt_caps()
    }

    /// Get orchestrator info (Kubernetes, etc.)
    pub fn orchestrator(&self) -> Option<OrchestratorInfo> {
        containers::detect_orchestrator()
    }

    /// Full snapshot
    pub fn snapshot(&self) -> VirtSnapshot {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        VirtSnapshot {
            platform: detect::detect_platform(),
            hypervisor: self.hypervisor(),
            container: self.container_info(),
            guest_resources: self.guest_resources(),
            cpu_capabilities: self.cpu_capabilities(),
            orchestrator: self.orchestrator(),
            timestamp: now,
        }
    }
}
