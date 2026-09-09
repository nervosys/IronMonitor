//! Apple Silicon monitoring via powermetrics and IOKit
//!
//! This module implements comprehensive monitoring for Apple Silicon (M1/M2/M3/M4 series)
//! using macOS's powermetrics utility and IOKit framework.
//!
//! Based on asitop: <https://github.com/tlkh/asitop>

use super::*;
use crate::error::{Error, Result};
use std::collections::HashMap;
use std::process::{Child, Command, Stdio};

#[cfg(all(feature = "apple", target_os = "macos"))]
use plist::Value;

/// Apple Silicon monitor
pub struct AppleSiliconMonitor {
    powermetrics_process: Option<Child>,
    soc_info: SocInfo,
}

/// SOC information
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct SocInfo {
    name: String,
    e_core_count: u32,
    p_core_count: u32,
    gpu_core_count: u32,
    cpu_max_power: f32,
    gpu_max_power: f32,
    ane_max_power: f32,
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

        Ok(Self {
            powermetrics_process: None,
            soc_info,
        })
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
        let gpu_cores = Self::get_gpu_cores().unwrap_or(0);

        // Determine power limits based on SOC
        let (cpu_max_power, gpu_max_power, ane_max_power) = match name.as_str() {
            s if s.contains("M1 Max") => (30.0, 60.0, 8.0),
            s if s.contains("M1 Pro") => (30.0, 30.0, 8.0),
            s if s.contains("M1 Ultra") => (60.0, 120.0, 16.0),
            s if s.contains("M2 Max") => (35.0, 65.0, 8.0),
            s if s.contains("M2 Pro") => (35.0, 35.0, 8.0),
            s if s.contains("M2 Ultra") => (70.0, 130.0, 16.0),
            s if s.contains("M2") => (25.0, 15.0, 8.0),
            s if s.contains("M3 Max") => (40.0, 70.0, 8.0),
            s if s.contains("M3 Pro") => (40.0, 40.0, 8.0),
            s if s.contains("M3") => (30.0, 20.0, 8.0),
            s if s.contains("M4 Max") => (45.0, 75.0, 10.0),
            s if s.contains("M4 Pro") => (45.0, 45.0, 10.0),
            s if s.contains("M4") => (35.0, 25.0, 10.0),
            s if s.contains("M1") => (20.0, 20.0, 8.0),
            _ => (20.0, 20.0, 8.0), // Default for unknown
        };

        Ok(SocInfo {
            name,
            e_core_count: e_cores,
            p_core_count: p_cores,
            gpu_core_count: gpu_cores,
            cpu_max_power,
            gpu_max_power,
            ane_max_power,
        })
    }

    /// Get sysctl value as u32
    fn get_sysctl_value(key: &str) -> Option<u32> {
        let output = Command::new("sysctl").args(["-n", key]).output().ok()?;

        String::from_utf8_lossy(&output.stdout).trim().parse().ok()
    }

    /// Get GPU core count from system_profiler
    fn get_gpu_cores() -> Option<u32> {
        let output = Command::new("system_profiler")
            .args(["-detailLevel", "basic", "SPDisplaysDataType"])
            .output()
            .ok()?;

        let text = String::from_utf8_lossy(&output.stdout);
        for line in text.lines() {
            if line.contains("Total Number of Cores") {
                if let Some(cores_str) = line.split(':').nth(1) {
                    return cores_str.trim().parse().ok();
                }
            }
        }
        None
    }

    /// Start powermetrics process
    #[allow(dead_code)]
    fn start_powermetrics(&mut self, interval_ms: u32) -> Result<()> {
        let temp_file = format!("/tmp/simon_powermetrics_{}", std::process::id());

        let child = Command::new("sudo")
            .args([
                "powermetrics",
                "--samplers",
                "cpu_power,gpu_power,thermal",
                "-o",
                &temp_file,
                "-f",
                "plist",
                "-i",
                &interval_ms.to_string(),
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| Error::CommandExecutionFailed(format!("powermetrics: {}", e)))?;

        self.powermetrics_process = Some(child);

        // Wait for first data
        std::thread::sleep(std::time::Duration::from_millis(interval_ms as u64 + 100));

        Ok(())
    }

    /// Parse powermetrics output
    pub fn parse_powermetrics(&self) -> Result<PowermetricsData> {
        #[cfg(all(feature = "apple", target_os = "macos"))]
        {
            let temp_file = format!("/tmp/simon_powermetrics_{}", std::process::id());

            // Read the plist file
            let data = std::fs::read(&temp_file).map_err(Error::Io)?;

            // Split by null bytes (powermetrics appends multiple plists)
            let parts: Vec<&[u8]> = data.split(|&b| b == 0).collect();

            // Parse the last complete plist
            if let Some(last_plist) = parts.iter().rev().find(|p| !p.is_empty()) {
                let value = Value::from_reader(std::io::Cursor::new(last_plist))
                    .map_err(|e| Error::ParseError(format!("plist: {}", e)))?;

                return self.parse_plist_data(&value);
            }
        }

        // Fallback to default data
        Ok(PowermetricsData::default())
    }

    #[cfg(all(feature = "apple", target_os = "macos"))]
    #[allow(dead_code)]
    /// Parse plist data into PowermetricsData
    fn parse_plist_data(&self, value: &Value) -> Result<PowermetricsData> {
        let mut data = PowermetricsData::default();

        if let Some(dict) = value.as_dictionary() {
            // Parse thermal pressure
            if let Some(Value::String(thermal)) = dict.get("thermal_pressure") {
                data.thermal_pressure = thermal.clone();
            }

            // Parse processor data
            if let Some(Value::Dictionary(processor)) = dict.get("processor") {
                // Parse clusters
                if let Some(Value::Array(clusters)) = processor.get("clusters") {
                    for cluster in clusters {
                        if let Some(cluster_dict) = cluster.as_dictionary() {
                            if let Some(Value::String(name)) = cluster_dict.get("name") {
                                // Get cluster metrics
                                let freq_hz = cluster_dict
                                    .get("freq_hz")
                                    .and_then(|v| v.as_unsigned_integer())
                                    .unwrap_or(0);
                                let freq_mhz = (freq_hz / 1_000_000) as u32;

                                let idle_ratio = cluster_dict
                                    .get("idle_ratio")
                                    .and_then(|v| v.as_real())
                                    .unwrap_or(1.0);
                                let active = ((1.0 - idle_ratio) * 100.0) as u8;

                                // Assign to E or P cluster
                                if name.starts_with('E') {
                                    data.e_cluster_freq_mhz = freq_mhz;
                                    data.e_cluster_active = active;
                                } else if name.starts_with('P') {
                                    data.p_cluster_freq_mhz = freq_mhz;
                                    data.p_cluster_active = active;
                                }

                                // Parse individual cores
                                if let Some(Value::Array(cpus)) = cluster_dict.get("cpus") {
                                    for cpu in cpus {
                                        if let Some(cpu_dict) = cpu.as_dictionary() {
                                            let cpu_num = cpu_dict
                                                .get("cpu")
                                                .and_then(|v| v.as_unsigned_integer())
                                                .unwrap_or(0)
                                                as u32;

                                            let cpu_freq_hz = cpu_dict
                                                .get("freq_hz")
                                                .and_then(|v| v.as_unsigned_integer())
                                                .unwrap_or(0);
                                            let cpu_freq_mhz = (cpu_freq_hz / 1_000_000) as u32;

                                            let cpu_idle = cpu_dict
                                                .get("idle_ratio")
                                                .and_then(|v| v.as_real())
                                                .unwrap_or(1.0);
                                            let cpu_active = ((1.0 - cpu_idle) * 100.0) as u8;

                                            if name.starts_with('E') {
                                                data.e_cores
                                                    .insert(cpu_num, (cpu_freq_mhz, cpu_active));
                                            } else {
                                                data.p_cores
                                                    .insert(cpu_num, (cpu_freq_mhz, cpu_active));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Parse power metrics
                if let Some(Value::Real(cpu_power)) = processor.get("cpu_energy") {
                    data.cpu_power_mw = *cpu_power as u32;
                }
                if let Some(Value::Real(gpu_power)) = processor.get("gpu_energy") {
                    data.gpu_power_mw = *gpu_power as u32;
                }
                if let Some(Value::Real(ane_power)) = processor.get("ane_energy") {
                    data.ane_power_mw = *ane_power as u32;
                }
                if let Some(Value::Real(package_power)) = processor.get("combined_power") {
                    data.package_power_mw = *package_power as u32;
                }
            }

            // Parse GPU data
            if let Some(Value::Dictionary(gpu)) = dict.get("gpu") {
                if let Some(Value::Integer(freq_hz)) = gpu.get("freq_hz") {
                    // `plist::Integer` is not a primitive; it must be asked for its value.
                    data.gpu_freq_mhz = (freq_hz.as_unsigned().unwrap_or(0) / 1_000_000) as u32;
                }
                if let Some(Value::Real(idle_ratio)) = gpu.get("idle_ratio") {
                    data.gpu_active = ((1.0 - idle_ratio) * 100.0) as u8;
                }
            }
        }

        Ok(data)
    }

    #[cfg(not(all(feature = "apple", target_os = "macos")))]
    #[allow(dead_code)]
    fn parse_plist_data(&self, _value: &()) -> Result<PowermetricsData> {
        Ok(PowermetricsData::default())
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
        clusters.push(CpuCluster {
            cluster_type: CpuClusterType::Efficiency,
            core_ids: (0..self.soc_info.e_core_count).collect(),
            frequency_mhz: Some(data.e_cluster_freq_mhz),
            utilization: Some(data.e_cluster_active),
            power_watts: Some(data.cpu_power_mw as f32 / 1000.0 * 0.4), // Approximate
        });

        // P-cluster
        clusters.push(CpuCluster {
            cluster_type: CpuClusterType::Performance,
            core_ids: (0..self.soc_info.p_core_count).collect(),
            frequency_mhz: Some(data.p_cluster_freq_mhz),
            utilization: Some(data.p_cluster_active),
            power_watts: Some(data.cpu_power_mw as f32 / 1000.0 * 0.6), // Approximate
        });

        Ok((cores, clusters))
    }

    fn npu_info(&self) -> Result<Vec<NpuInfo>> {
        let data = self.parse_powermetrics()?;

        // ANE utilization is estimated from power consumption
        let ane_util =
            ((data.ane_power_mw as f32 / 1000.0) / self.soc_info.ane_max_power * 100.0) as u8;

        Ok(vec![NpuInfo {
            name: "Apple Neural Engine".to_string(),
            vendor: "Apple".to_string(),
            // "Most" is not this machine. The ANE core count is not read.
            cores: None,
            // Derived from power draw, not measured directly — the comment
            // above says so, and the figure is real.
            utilization: Some(ane_util),
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

impl Drop for AppleSiliconMonitor {
    fn drop(&mut self) {
        if let Some(mut child) = self.powermetrics_process.take() {
            let _ = child.kill();
        }
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
