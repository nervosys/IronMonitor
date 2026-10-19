//! Apple Silicon monitoring via powermetrics and IOKit
//!
//! This module implements comprehensive monitoring for Apple Silicon (M1/M2/M3/M4 series)
//! using macOS's powermetrics utility and IOKit framework.
//!
//! Based on asitop: <https://github.com/tlkh/asitop>

use super::*;
use crate::error::{Error, Result};
use std::collections::HashMap;
use std::process::Command;

/// Apple Silicon monitor
pub struct AppleSiliconMonitor {
    soc_info: SocInfo,
}

/// SOC information
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct SocInfo {
    name: String,
    e_core_count: u32,
    p_core_count: u32,
}

/// Powermetrics data
#[derive(Debug, Default)]
pub struct PowermetricsData {
    // CPU metrics
    pub e_cluster_freq_mhz: u32,
    pub e_cluster_active: u8,
    pub p_cluster_freq_mhz: u32,
    pub p_cluster_active: u8,

    // Per-core metrics
    pub e_cores: HashMap<u32, (u32, u8)>, // (freq, utilization)
    pub p_cores: HashMap<u32, (u32, u8)>,

    // GPU metrics
    pub gpu_freq_mhz: u32,
    pub gpu_active: u8,

    // Power metrics
    pub cpu_power_mw: u32,
    pub gpu_power_mw: u32,
    pub ane_power_mw: u32,
    pub package_power_mw: u32,

    // Thermal
    pub thermal_pressure: String,
}

impl AppleSiliconMonitor {
    /// Create new Apple Silicon monitor
    pub fn new() -> Result<Self> {
        let soc_info = Self::detect_soc_info()?;

        Ok(Self { soc_info })
    }

    /// Detect SOC information using sysctl and system_profiler
    fn detect_soc_info() -> Result<SocInfo> {
        // Get CPU brand string
        let output = Command::new("sysctl")
            .args(["-n", "machdep.cpu.brand_string"])
            .output()
            .map_err(|e| Error::CommandExecutionFailed(format!("sysctl: {}", e)))?;

        let name = String::from_utf8_lossy(&output.stdout).trim().to_string();

        // Core counts, or zero where the sysctl did not answer.
        //
        // These fell back to `4`, `4` and `8` -- **a specific and entirely
        // plausible machine**, which is what makes the fabrication worse than
        // the usual one. An M1 is 4+4 with 8 GPU cores, so a failed read
        // described the hardware as a base M1 whatever it actually was, and
        // nothing downstream could tell the difference: `core_ids` came out as
        // `0..4` and looked like a topology somebody had measured.
        //
        // A `1` is implausible enough to be noticed. A `4` is not, and that is
        // the argument against "sensible defaults" in a reader: the more
        // reasonable the invented value, the longer it survives.
        //
        // Zero here means the count was not read. `(0..0)` yields an empty
        // `core_ids`, and the cluster still carries the frequency and
        // utilisation `powermetrics` actually measured -- an unknown
        // composition, not an unknown cluster.
        let e_cores = Self::get_sysctl_value("hw.perflevel1.logicalcpu").unwrap_or(0);
        let p_cores = Self::get_sysctl_value("hw.perflevel0.logicalcpu").unwrap_or(0);

        // There was a per-chip power table here -- CPU, GPU and ANE ceilings
        // by brand-string match, with `(20.0, 20.0, 8.0)` for any chip it did
        // not name. Its only reader divided the ANE's measured draw by that
        // ceiling and reported the result as utilisation; see `npu_info`. A GPU
        // core count read from `system_profiler` sat beside it, stored and
        // never read. Both are gone, and with them a `system_profiler` spawn on
        // every construction.
        Ok(SocInfo {
            name,
            e_core_count: e_cores,
            p_core_count: p_cores,
        })
    }

    /// Get sysctl value as u32
    fn get_sysctl_value(key: &str) -> Option<u32> {
        let output = Command::new("sysctl").args(["-n", key]).output().ok()?;

        String::from_utf8_lossy(&output.stdout).trim().parse().ok()
    }

    /// The live CPU, GPU and ANE figures `powermetrics` reports.
    ///
    /// Always an error, and the error says why. `powermetrics` must run as
    /// root, and IronMonitor does not elevate itself — AGENTS.md promises
    /// that, and an interactive `sudo` would sit waiting for a password inside
    /// a TUI or an agent's tool call.
    ///
    /// What was here could never succeed and could mislead if it did. It read
    /// a plist from `/tmp/ironmon_powermetrics_<pid>`, a file written only by
    /// a `sudo powermetrics` launcher that nothing called; on macOS it failed
    /// with a bare "file not found", and elsewhere it returned a default — every
    /// clock, utilisation and power figure zero — as a successful reading. Its
    /// parser filled each missing key with 0 MHz, 0% busy or core 0, and its
    /// key names were never checked against a real `powermetrics` sample.
    ///
    /// A reader that runs `powermetrics` directly when the process is already
    /// root belongs here, written against output from a real Mac.
    pub fn parse_powermetrics(&self) -> Result<PowermetricsData> {
        Err(crate::error::IronError::PermissionDenied(
            "live Apple Silicon CPU, GPU and ANE figures come from powermetrics, \
             which requires root; IronMonitor does not elevate itself"
                .into(),
        ))
    }
}

impl SiliconMonitor for AppleSiliconMonitor {
    fn cpu_info(&self) -> Result<(Vec<CpuCore>, Vec<CpuCluster>)> {
        let data = self.parse_powermetrics()?;

        let mut cores = Vec::new();
        let mut clusters = Vec::new();

        // E-cores
        for (id, (freq, util)) in &data.e_cores {
            cores.push(CpuCore {
                id: *id,
                cluster: CpuClusterType::Efficiency,
                frequency_mhz: Some(*freq),
                utilization: Some(*util),
                temperature: None,
            });
        }

        // P-cores
        for (id, (freq, util)) in &data.p_cores {
            cores.push(CpuCore {
                id: *id,
                cluster: CpuClusterType::Performance,
                frequency_mhz: Some(*freq),
                utilization: Some(*util),
                temperature: None,
            });
        }

        // E-cluster. An empty `core_ids` means the per-level core count was
        // not read, not that the cluster has no cores; the figures beside it
        // are measured either way.
        //
        // Neither cluster reports power. `powermetrics` gives one CPU figure,
        // and this used to split it 40/60 between the clusters -- a fixed
        // ratio presented as two per-cluster readings, the E-cluster's
        // unchanged whether it was idle or saturated.
        clusters.push(CpuCluster {
            cluster_type: CpuClusterType::Efficiency,
            core_ids: (0..self.soc_info.e_core_count).collect(),
            frequency_mhz: Some(data.e_cluster_freq_mhz),
            utilization: Some(data.e_cluster_active),
            power_watts: None,
        });

        // P-cluster
        clusters.push(CpuCluster {
            cluster_type: CpuClusterType::Performance,
            core_ids: (0..self.soc_info.p_core_count).collect(),
            frequency_mhz: Some(data.p_cluster_freq_mhz),
            utilization: Some(data.p_cluster_active),
            power_watts: None,
        });

        Ok((cores, clusters))
    }

    fn npu_info(&self) -> Result<Vec<NpuInfo>> {
        let data = self.parse_powermetrics()?;

        Ok(vec![NpuInfo {
            name: "Apple Neural Engine".to_string(),
            vendor: "Apple".to_string(),
            // "Most" is not this machine. The ANE core count is not read.
            cores: None,
            // Not reported. This was the ANE's measured draw divided by a
            // ceiling from a brand-string table -- 8 W for any chip the table
            // did not name -- which is the GPU power-percentage defect over
            // again: a real numerator over an invented denominator, reaching
            // the TUI's accelerator panel as a utilisation. `powermetrics`
            // reports no ANE activity figure, so there is nothing to put here.
            utilization: None,
            power_watts: Some(data.ane_power_mw as f32 / 1000.0),
            frequency_mhz: None,
        }])
    }

    fn io_info(&self) -> Result<Vec<IoController>> {
        let mut controllers = Vec::new();

        // Get disk I/O stats from iostat
        if let Ok(output) = Command::new("iostat").args(["-d", "-c", "1"]).output() {
            let text = String::from_utf8_lossy(&output.stdout);
            // iostat -d outputs: device, KB/t, tps, MB/s
            for line in text.lines().skip(2) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 4 {
                    let name = parts[0].to_string();
                    let mbps: f64 = parts[3].parse().unwrap_or(0.0);
                    controllers.push(IoController {
                        controller_type: if name.starts_with("disk") {
                            "NVMe".to_string()
                        } else {
                            "Storage".to_string()
                        },
                        name,
                        bandwidth_mbps: Some(mbps),
                        // 7000 was the spec figure for Apple Silicon's fastest
                        // NVMe, applied to whatever this disk is. Nothing about
                        // the actual link is read here.
                        max_bandwidth_mbps: None,
                        power_watts: None,
                    });
                }
            }
        }

        // Thunderbolt controllers via system_profiler
        if let Ok(output) = Command::new("system_profiler")
            .args(["-detailLevel", "mini", "SPThunderboltDataType"])
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            let mut tb_count = 0u32;
            for line in text.lines() {
                if (line.trim().starts_with("Speed:") || line.trim().contains("Thunderbolt"))
                    && line.contains("Up to")
                {
                    tb_count += 1;
                    controllers.push(IoController {
                        controller_type: "Thunderbolt".to_string(),
                        name: format!("Thunderbolt Port {}", tb_count),
                        // The *current* bandwidth was set to the theoretical
                        // maximum, so an idle Thunderbolt port reported 5000
                        // MB/s of traffic — a fabricated saturated bus, not a
                        // sentinel zero. Nothing here reads either figure.
                        bandwidth_mbps: None,
                        max_bandwidth_mbps: None,
                        power_watts: None,
                    });
                }
            }
        }

        Ok(controllers)
    }

    fn network_info(&self) -> Result<Vec<NetworkSilicon>> {
        let mut networks = Vec::new();

        // Get active network interfaces via ifconfig
        if let Ok(output) = Command::new("ifconfig").output() {
            let text = String::from_utf8_lossy(&output.stdout);
            let mut current_iface = String::new();
            let mut is_up = false;

            for line in text.lines() {
                if !line.starts_with('\t') && !line.starts_with(' ') && line.contains(':') {
                    // New interface line like "en0: flags=8863<UP,...>"
                    let iface = line.split(':').next().unwrap_or("").to_string();
                    if !current_iface.is_empty() && is_up && !current_iface.starts_with("lo") {
                        let interface_type = classify_macos_interface(&current_iface);
                        networks.push(NetworkSilicon {
                            interface: interface_type,
                            link_speed_mbps: None,
                            rx_bandwidth_mbps: 0.0,
                            tx_bandwidth_mbps: 0.0,
                            packet_rate: 0,
                            power_state: Some("active".to_string()),
                        });
                    }
                    current_iface = iface;
                    is_up = line.contains("UP");
                }
            }
            // Push last
            if !current_iface.is_empty() && is_up && !current_iface.starts_with("lo") {
                let interface_type = classify_macos_interface(&current_iface);
                networks.push(NetworkSilicon {
                    interface: interface_type,
                    link_speed_mbps: None,
                    rx_bandwidth_mbps: 0.0,
                    tx_bandwidth_mbps: 0.0,
                    packet_rate: 0,
                    power_state: Some("active".to_string()),
                });
            }
        }

        // Get bandwidth from netstat
        if let Ok(output) = Command::new("netstat").args(["-ib"]).output() {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines().skip(1) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 7 {
                    let iface = parts[0];
                    // Find matching network entry and update bytes
                    if let Some(net) = networks.iter_mut().find(|n| {
                        (n.interface == "WiFi" && iface == "en0")
                            || (n.interface == "Ethernet" && iface.starts_with("en"))
                    }) {
                        if let Ok(ibytes) = parts[6].parse::<u64>() {
                            net.rx_bandwidth_mbps = ibytes as f64 / (1024.0 * 1024.0);
                        }
                        if parts.len() > 9 {
                            if let Ok(obytes) = parts[9].parse::<u64>() {
                                net.tx_bandwidth_mbps = obytes as f64 / (1024.0 * 1024.0);
                            }
                        }
                    }
                }
            }
        }

        Ok(networks)
    }
}

/// Classify macOS interface name to type
fn classify_macos_interface(iface: &str) -> String {
    if iface == "en0" {
        "WiFi".to_string()
    } else if iface.starts_with("en") {
        "Ethernet".to_string()
    } else if iface.starts_with("bridge") {
        "Bridge".to_string()
    } else if iface.starts_with("awdl") {
        "AirDrop".to_string()
    } else if iface.starts_with("utun") {
        "VPN Tunnel".to_string()
    } else {
        iface.to_string()
    }
}

// `estimate_link_speed` was here, and its own doc comment said what was wrong
// with it: "These are guesses keyed off the interface name, not measurements:
// `en0` is assumed to be Wi-Fi 6 and anything else `en*`/`bridge*` gigabit
// Ethernet. Callers must not present the result as a read link rate."
//
// The caller two lines away assigned it straight to `link_speed_mbps`. A
// warning in a doc comment does not bind the caller; a type does. The field is
// `Option<u32>` and reads `None` here until something calls `IOKit` for the
// interface's active media type.

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;

    /// Without root there is no `powermetrics` sample, and the readers built
    /// on it say so rather than reporting a machine at 0 MHz and 0 mW.
    #[test]
    fn live_figures_are_refused_with_the_reason_not_zeroed() {
        let monitor = AppleSiliconMonitor {
            soc_info: SocInfo {
                name: "Apple M2".into(),
                e_core_count: 4,
                p_core_count: 4,
            },
        };
        let err = monitor.parse_powermetrics().expect_err("no sample exists");
        assert!(
            matches!(err, crate::error::IronError::PermissionDenied(ref why) if why.contains("root")),
            "{err:?}"
        );
        assert!(monitor.cpu_info().is_err());
        assert!(monitor.npu_info().is_err());
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_detect_soc() {
        let soc = AppleSiliconMonitor::detect_soc_info();
        if let Ok(soc_info) = soc {
            println!("Detected SOC: {}", soc_info.name);
            println!(
                "E-cores: {}, P-cores: {}",
                soc_info.e_core_count, soc_info.p_core_count
            );
        }
    }
}
