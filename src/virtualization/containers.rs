// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2026 nervosys

//! Container runtime detection and resource monitoring

use serde::{Deserialize, Serialize};

/// Container engine types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContainerEngine {
    Docker,
    Podman,
    Containerd,
    CriO,
    Lxc,
    Lxd,
    Rkt,
    Kata,
    Nerdctl,
    Unknown,
}

/// Cgroup version
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CgroupVersion {
    V1,
    V2,
    Unknown,
}

/// Container resource limits from cgroups
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerResources {
    pub cgroup_version: CgroupVersion,
    pub cpu_quota: Option<i64>,
    pub cpu_period: Option<u64>,
    /// cgroup v1 `cpu.shares` (default 1024). Never holds a v2 value: the v2
    /// `cpu.weight` (default 100) is on a different scale, and used to be
    /// stored here as if it were shares.
    pub cpu_shares: Option<u64>,
    /// cgroup v2 `cpu.weight` (default 100), `None` on v1.
    pub cpu_weight: Option<u64>,
    pub memory_limit_bytes: Option<u64>,
    pub memory_usage_bytes: Option<u64>,
    pub pids_limit: Option<u64>,
    pub pids_current: Option<u64>,
}

/// Container info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerInfo {
    pub engine: ContainerEngine,
    pub container_id: Option<String>,
    pub resources: Option<ContainerResources>,
    /// `None` when neither `HOSTNAME` nor the `hostname` command gave one. This
    /// was `"unknown"`, a string that reads as a hostname.
    pub hostname: Option<String>,
}

/// Kubernetes pod info (from Downward API)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct K8sPodInfo {
    pub pod_name: Option<String>,
    pub pod_namespace: Option<String>,
    pub pod_uid: Option<String>,
    pub node_name: Option<String>,
    pub service_account: Option<String>,
    pub has_service_account_token: bool,
}

/// Orchestrator types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrchestratorType {
    Kubernetes,
    DockerSwarm,
    Nomad,
    Mesos,
    Unknown,
}

/// Orchestrator info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorInfo {
    pub orchestrator: OrchestratorType,
    pub pod_info: Option<K8sPodInfo>,
}

/// Detect if running inside a container
pub fn detect_container() -> Option<ContainerInfo> {
    #[cfg(target_os = "linux")]
    {
        detect_linux_container()
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

#[cfg(target_os = "linux")]
fn detect_linux_container() -> Option<ContainerInfo> {
    use std::path::Path;

    let engine = if Path::new("/.dockerenv").exists() {
        ContainerEngine::Docker
    } else if Path::new("/run/.containerenv").exists() {
        ContainerEngine::Podman
    } else if is_in_cgroup_container() {
        detect_engine_from_cgroup()
    } else {
        return None;
    };

    let container_id = extract_container_id();
    let resources = detect_cgroup_resources();
    let hostname = hostname();

    Some(ContainerInfo {
        engine,
        container_id,
        resources,
        hostname,
    })
}

#[cfg(target_os = "linux")]
fn is_in_cgroup_container() -> bool {
    std::fs::read_to_string("/proc/1/cgroup")
        .map(|c| {
            c.contains("docker")
                || c.contains("containerd")
                || c.contains("lxc")
                || c.contains("kubepods")
                || c.contains("crio")
        })
        .unwrap_or(false)
}

#[cfg(target_os = "linux")]
fn detect_engine_from_cgroup() -> ContainerEngine {
    let cgroup = std::fs::read_to_string("/proc/1/cgroup").unwrap_or_default();
    if cgroup.contains("docker") {
        ContainerEngine::Docker
    } else if cgroup.contains("crio") || cgroup.contains("cri-o") {
        ContainerEngine::CriO
    } else if cgroup.contains("containerd") {
        ContainerEngine::Containerd
    } else if cgroup.contains("lxc") {
        ContainerEngine::Lxc
    } else {
        ContainerEngine::Unknown
    }
}

#[cfg(target_os = "linux")]
fn extract_container_id() -> Option<String> {
    let cgroup = std::fs::read_to_string("/proc/1/cgroup").ok()?;
    for line in cgroup.lines() {
        let parts: Vec<&str> = line.split('/').collect();
        if let Some(last) = parts.last() {
            let id = last.trim();
            if id.len() >= 12 && id.chars().all(|c| c.is_ascii_hexdigit()) {
                return Some(id[..12].to_string());
            }
        }
    }
    // Try mountinfo
    let mountinfo = std::fs::read_to_string("/proc/self/mountinfo").ok()?;
    for line in mountinfo.lines() {
        if let Some(pos) = line.find("/docker/containers/") {
            let rest = &line[pos + 19..];
            if rest.len() >= 12 {
                return Some(rest[..12].to_string());
            }
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn detect_cgroup_resources() -> Option<ContainerResources> {
    let version = detect_cgroup_version();
    match version {
        CgroupVersion::V2 => detect_cgroupv2(),
        CgroupVersion::V1 => detect_cgroupv1(),
        CgroupVersion::Unknown => None,
    }
}

#[cfg(target_os = "linux")]
fn detect_cgroup_version() -> CgroupVersion {
    use std::path::Path;
    if Path::new("/sys/fs/cgroup/cgroup.controllers").exists() {
        CgroupVersion::V2
    } else if Path::new("/sys/fs/cgroup/cpu/cpu.cfs_quota_us").exists() {
        CgroupVersion::V1
    } else {
        CgroupVersion::Unknown
    }
}

#[cfg(target_os = "linux")]
fn detect_cgroupv2() -> Option<ContainerResources> {
    use std::fs;
    let base = "/sys/fs/cgroup";
    let _read_i64 = |name: &str| -> Option<i64> {
        fs::read_to_string(format!("{}/{}", base, name))
            .ok()
            .and_then(|v| v.trim().parse().ok())
    };
    let read_u64 = |name: &str| -> Option<u64> {
        fs::read_to_string(format!("{}/{}", base, name))
            .ok()
            .and_then(|v| v.trim().parse().ok())
    };

    // "max" is unlimited, a value read: -1 for the quota, u64::MAX for a limit.
    // Only an unreadable or unparseable file is None.
    let read_limit = |name: &str| -> Option<u64> {
        fs::read_to_string(format!("{}/{}", base, name))
            .ok()
            .and_then(|v| crate::cgroup_monitor::parse_limit(&v))
    };

    let (cpu_quota, cpu_period) = fs::read_to_string(format!("{}/cpu.max", base))
        .ok()
        .and_then(|v| crate::cgroup_monitor::parse_cpu_max(&v))
        .map_or((None, None), |(q, p)| (Some(q), Some(p)));

    Some(ContainerResources {
        cgroup_version: CgroupVersion::V2,
        cpu_quota,
        cpu_period,
        cpu_shares: None,
        cpu_weight: read_u64("cpu.weight"),
        memory_limit_bytes: read_limit("memory.max"),
        memory_usage_bytes: read_u64("memory.current"),
        pids_limit: read_limit("pids.max"),
        pids_current: read_u64("pids.current"),
    })
}

#[cfg(target_os = "linux")]
fn detect_cgroupv1() -> Option<ContainerResources> {
    use std::fs;
    let read_i64 = |path: &str| -> Option<i64> {
        fs::read_to_string(path)
            .ok()
            .and_then(|v| v.trim().parse().ok())
    };
    let read_u64 = |path: &str| -> Option<u64> {
        fs::read_to_string(path)
            .ok()
            .and_then(|v| v.trim().parse().ok())
    };

    Some(ContainerResources {
        cgroup_version: CgroupVersion::V1,
        cpu_quota: read_i64("/sys/fs/cgroup/cpu/cpu.cfs_quota_us"),
        cpu_period: read_u64("/sys/fs/cgroup/cpu/cpu.cfs_period_us"),
        cpu_shares: read_u64("/sys/fs/cgroup/cpu/cpu.shares"),
        cpu_weight: None,
        memory_limit_bytes: read_u64("/sys/fs/cgroup/memory/memory.limit_in_bytes"),
        memory_usage_bytes: read_u64("/sys/fs/cgroup/memory/memory.usage_in_bytes"),
        pids_limit: read_u64("/sys/fs/cgroup/pids/pids.max"),
        pids_current: read_u64("/sys/fs/cgroup/pids/pids.current"),
    })
}

/// Detect orchestrator
pub fn detect_orchestrator() -> Option<OrchestratorInfo> {
    // Check Kubernetes
    if std::env::var("KUBERNETES_SERVICE_HOST").is_ok() {
        let pod_info = K8sPodInfo {
            pod_name: std::env::var("HOSTNAME")
                .ok()
                .or_else(|| std::env::var("POD_NAME").ok()),
            pod_namespace: std::env::var("POD_NAMESPACE").ok(),
            pod_uid: std::env::var("POD_UID").ok(),
            node_name: std::env::var("NODE_NAME").ok(),
            service_account: std::env::var("SERVICE_ACCOUNT").ok(),
            has_service_account_token: std::path::Path::new(
                "/var/run/secrets/kubernetes.io/serviceaccount/token",
            )
            .exists(),
        };
        return Some(OrchestratorInfo {
            orchestrator: OrchestratorType::Kubernetes,
            pod_info: Some(pod_info),
        });
    }

    // Check Docker Swarm
    if std::env::var("DOCKER_SWARM_SERVICE").is_ok() || std::env::var("SWARM_NODE_ID").is_ok() {
        return Some(OrchestratorInfo {
            orchestrator: OrchestratorType::DockerSwarm,
            pod_info: None,
        });
    }

    // Check Nomad
    if std::env::var("NOMAD_ALLOC_ID").is_ok() {
        return Some(OrchestratorInfo {
            orchestrator: OrchestratorType::Nomad,
            pod_info: None,
        });
    }

    // Check Mesos
    if std::env::var("MESOS_TASK_ID").is_ok() {
        return Some(OrchestratorInfo {
            orchestrator: OrchestratorType::Mesos,
            pod_info: None,
        });
    }

    None
}

/// The hostname, or `None` when neither source gives a non-empty one. This used
/// to return `"unknown"` in that case, which reads as a hostname.
#[allow(dead_code)]
fn hostname() -> Option<String> {
    let non_empty = |h: String| Some(h.trim().to_string()).filter(|h| !h.is_empty());
    std::env::var("HOSTNAME")
        .ok()
        .and_then(non_empty)
        .or_else(|| {
            std::process::Command::new("hostname")
                .output()
                .ok()
                .and_then(|o| non_empty(String::from_utf8_lossy(&o.stdout).into_owned()))
        })
}
