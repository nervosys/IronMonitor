//! CPU interconnect topology monitoring.
//!
//! Detects and characterizes CPU-to-CPU and CPU-to-device interconnects:
//! Intel QPI/UPI, AMD Infinity Fabric, PCIe root complexes, and coherence
//! domains. Uses inference from CPU model to estimate link bandwidth and
//! topology when direct counters aren't available.
//!
//! ## Platform Support
//!
//! - **Linux**: `/sys/devices/system/node/`, lscpu topology, CPU model inference
//! - **Windows**: Win32_Processor socket info, CPU model inference
//! - **macOS**: sysctl, CPU model inference

use serde::{Deserialize, Serialize};

use crate::error::IronError;

/// Interconnect technology type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InterconnectType {
    /// Intel QuickPath Interconnect (Nehalem–Broadwell).
    QPI,
    /// Intel Ultra Path Interconnect (Skylake-SP+).
    UPI,
    /// AMD Infinity Fabric (Zen+).
    InfinityFabric,
    /// AMD Infinity Fabric on-package (chiplet-to-chiplet).
    InfinityFabricOnPackage,
    /// Apple Unified Memory Architecture interconnect.
    AppleUMA,
    /// ARM AMBA / CoreLink / CMN mesh.
    ArmMesh,
    /// PCIe root complex interconnect.
    PCIe,
    /// CXL (Compute Express Link).
    CXL,
    /// Intel on-die ring bus.
    RingBus,
    /// Intel mesh interconnect.
    MeshInterconnect,
    /// Unknown interconnect.
    Unknown,
}

impl std::fmt::Display for InterconnectType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::QPI => write!(f, "Intel QPI"),
            Self::UPI => write!(f, "Intel UPI"),
            Self::InfinityFabric => write!(f, "AMD Infinity Fabric"),
            Self::InfinityFabricOnPackage => write!(f, "AMD Infinity Fabric (on-package)"),
            Self::AppleUMA => write!(f, "Apple UMA"),
            Self::ArmMesh => write!(f, "ARM Mesh"),
            Self::PCIe => write!(f, "PCIe"),
            Self::CXL => write!(f, "CXL"),
            Self::RingBus => write!(f, "Intel Ring Bus"),
            Self::MeshInterconnect => write!(f, "Intel Mesh"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Coherence protocol type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CoherenceProtocol {
    /// MOESI (AMD).
    MOESI,
    /// MESIF (Intel).
    MESIF,
    /// MESI (generic).
    MESI,
    /// Directory-based (ARM, large Intel).
    Directory,
    /// Snooping.
    Snoop,
    Unknown,
}

impl std::fmt::Display for CoherenceProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MOESI => write!(f, "MOESI"),
            Self::MESIF => write!(f, "MESIF"),
            Self::MESI => write!(f, "MESI"),
            Self::Directory => write!(f, "Directory-based"),
            Self::Snoop => write!(f, "Snoop"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

/// An interconnect link between two endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterconnectLink {
    /// Link type.
    pub link_type: InterconnectType,
    /// Source endpoint description (e.g. "Socket 0", "CCD 0").
    pub source: String,
    /// Destination endpoint description.
    pub destination: String,
    /// Unidirectional bandwidth in GB/s.
    pub bandwidth_gbs: f64,
    /// Bidirectional bandwidth in GB/s.
    pub bidirectional_bandwidth_gbs: f64,
    /// Link width (number of lanes/links).
    pub width: u32,
    /// Speed per lane in GT/s (gigatransfers/second).
    pub speed_gts: f64,
    /// Latency in nanoseconds (estimated).
    pub latency_ns: f64,
    /// Whether this link is active/detected.
    pub active: bool,
}

/// Chiplet/die topology for multi-die packages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChipletTopology {
    /// Number of compute dies/chiplets per package.
    pub compute_dies: u32,
    /// Number of I/O dies per package.
    pub io_dies: u32,
    /// Cores per compute die.
    /// Cores per compute die, where the physical core count is readable.
    ///
    /// Derived as `physical_cores / compute_dies`, not taken from a table. The
    /// table held the *maximum* CCD size for the generation — 8 for Zen 4 and
    /// Zen 5 — so a Ryzen 9 9900X, which is two partially-enabled CCDs of six,
    /// was reported as 2 x 8. Every partially-enabled part in the range had the
    /// same problem.
    pub cores_per_die: Option<u32>,
    /// On-package interconnect type.
    pub on_package_interconnect: InterconnectType,
    /// On-package bandwidth per link in GB/s.
    pub on_package_bandwidth_gbs: f64,
}

/// Full interconnect topology report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterconnectTopology {
    /// Number of CPU sockets.
    pub sockets: u32,
    /// Inter-socket links.
    pub links: Vec<InterconnectLink>,
    /// Chiplet topology (if applicable).
    pub chiplet_topology: Option<ChipletTopology>,
    /// Coherence protocol in use.
    pub coherence_protocol: CoherenceProtocol,
    /// Total system interconnect bandwidth in GB/s.
    pub total_bandwidth_gbs: f64,
    /// Whether the system is NUMA.
    pub is_numa: bool,
    /// Inferred interconnect generation/version.
    pub generation: String,
}

/// Interconnect monitor.
pub struct InterconnectMonitor {
    topology: InterconnectTopology,
}

impl InterconnectMonitor {
    /// Create a new interconnect monitor.
    pub fn new() -> Result<Self, IronError> {
        let cpu_model = Self::get_cpu_model()?;
        let sockets = Self::detect_sockets()?;
        let physical_cores = Self::detect_physical_cores();
        let topology = Self::infer_topology(&cpu_model, sockets, physical_cores);
        Ok(Self { topology })
    }

    /// Get the topology report.
    pub fn topology(&self) -> &InterconnectTopology {
        &self.topology
    }

    /// Get inter-socket links only.
    pub fn inter_socket_links(&self) -> Vec<&InterconnectLink> {
        self.topology.links.iter().filter(|l| l.active).collect()
    }

    /// Infer full topology from CPU model and socket count.
    fn infer_topology(
        cpu_model: &str,
        sockets: u32,
        physical_cores: Option<u32>,
    ) -> InterconnectTopology {
        let upper = cpu_model.to_uppercase();

        // Determine vendor and product family.
        //
        // `is_intel` tested `contains("CORE")` and was checked first. AMD
        // writes the core count into the model string — "AMD Ryzen 9 9900X
        // **12-Core** Processor" — so every Ryzen, EPYC and Threadripper
        // matched it and was given Intel's ring-bus topology with MESIF
        // coherence. On this desktop the module described a Zen 5 chiplet part
        // as a ring bus and never reached `infer_amd` at all.
        //
        // "CORE" was meant to catch "Core i7" and "Core Ultra"; both are still
        // caught, by tokens that a core count cannot produce. AMD is tested
        // first regardless, because its own name is unambiguous.
        let is_amd = upper.contains("AMD")
            || upper.contains("RYZEN")
            || upper.contains("EPYC")
            || upper.contains("THREADRIPPER");
        let is_intel = !is_amd
            && (upper.contains("INTEL")
                || upper.contains("XEON")
                || upper.contains("CORE I")
                || upper.contains("CORE ULTRA")
                || upper.contains("CELERON")
                || upper.contains("PENTIUM"));
        let is_apple = upper.contains("APPLE")
            || upper.contains("M1")
            || upper.contains("M2")
            || upper.contains("M3")
            || upper.contains("M4");

        if is_amd {
            Self::infer_amd(&upper, sockets, physical_cores)
        } else if is_intel {
            Self::infer_intel(&upper, sockets)
        } else if is_apple {
            Self::infer_apple(&upper)
        } else {
            InterconnectTopology {
                sockets,
                links: Vec::new(),
                chiplet_topology: None,
                coherence_protocol: CoherenceProtocol::Unknown,
                total_bandwidth_gbs: 0.0,
                is_numa: sockets > 1,
                generation: "Unknown".into(),
            }
        }
    }

    fn infer_intel(name: &str, sockets: u32) -> InterconnectTopology {
        // Determine Intel interconnect generation
        let (link_type, speed_gts, width, gen, on_die, coherence) =
            if name.contains("GRANITE") || name.contains("EMERALD") || name.contains("SIERRA") {
                (
                    InterconnectType::UPI,
                    20.0,
                    3,
                    "UPI 2.0",
                    InterconnectType::MeshInterconnect,
                    CoherenceProtocol::MESIF,
                )
            } else if name.contains("SAPPHIRE") || name.contains("W9-3") || name.contains("W7-3") {
                (
                    InterconnectType::UPI,
                    16.0,
                    3,
                    "UPI 1.1",
                    InterconnectType::MeshInterconnect,
                    CoherenceProtocol::MESIF,
                )
            } else if name.contains("ICE LAKE") && name.contains("XEON") {
                (
                    InterconnectType::UPI,
                    11.2,
                    3,
                    "UPI 1.0",
                    InterconnectType::MeshInterconnect,
                    CoherenceProtocol::MESIF,
                )
            } else if name.contains("XEON") && (name.contains("PLATINUM") || name.contains("GOLD"))
            {
                (
                    InterconnectType::UPI,
                    10.4,
                    3,
                    "UPI 1.0",
                    InterconnectType::MeshInterconnect,
                    CoherenceProtocol::MESIF,
                )
            } else if name.contains("12TH")
                || name.contains("13TH")
                || name.contains("14TH")
                || name.contains("ALDER")
                || name.contains("RAPTOR")
            {
                (
                    InterconnectType::RingBus,
                    0.0,
                    0,
                    "Ring Bus",
                    InterconnectType::RingBus,
                    CoherenceProtocol::MESIF,
                )
            } else if name.contains("ARROW") || name.contains("LUNAR") || name.contains("ULTRA") {
                (
                    InterconnectType::RingBus,
                    0.0,
                    0,
                    "Ring/Foveros",
                    InterconnectType::RingBus,
                    CoherenceProtocol::MESIF,
                )
            } else {
                (
                    InterconnectType::Unknown,
                    0.0,
                    0,
                    "Unknown",
                    InterconnectType::RingBus,
                    CoherenceProtocol::MESIF,
                )
            };

        // Bandwidth: speed (GT/s) * width (bytes) * links
        let uni_bw = speed_gts * width as f64 * 2.0; // 2 bytes per transfer on UPI
        let bidi_bw = uni_bw * 2.0;

        let mut links = Vec::new();
        if sockets > 1 && speed_gts > 0.0 {
            // Create inter-socket links
            for i in 0..sockets {
                for j in (i + 1)..sockets {
                    links.push(InterconnectLink {
                        link_type,
                        source: format!("Socket {}", i),
                        destination: format!("Socket {}", j),
                        bandwidth_gbs: uni_bw,
                        bidirectional_bandwidth_gbs: bidi_bw,
                        width,
                        speed_gts,
                        latency_ns: if speed_gts >= 16.0 { 60.0 } else { 80.0 },
                        active: true,
                    });
                }
            }
        }

        // On-die interconnect
        links.push(InterconnectLink {
            link_type: on_die,
            source: "Die".into(),
            destination: "Cores/LLC".into(),
            bandwidth_gbs: 0.0, // On-die bandwidth varies
            bidirectional_bandwidth_gbs: 0.0,
            width: 0,
            speed_gts: 0.0,
            latency_ns: if matches!(on_die, InterconnectType::MeshInterconnect) {
                15.0
            } else {
                10.0
            },
            active: true,
        });

        let total_bw: f64 = links.iter().map(|l| l.bidirectional_bandwidth_gbs).sum();

        InterconnectTopology {
            sockets,
            links,
            chiplet_topology: None,
            coherence_protocol: coherence,
            total_bandwidth_gbs: total_bw,
            is_numa: sockets > 1,
            generation: gen.into(),
        }
    }

    fn infer_amd(name: &str, sockets: u32, physical_cores: Option<u32>) -> InterconnectTopology {
        // AMD Infinity Fabric inference
        let (if_speed_gts, compute_dies, io_dies, _max_cores_per_die, gen, on_pkg_bw) = if name
            .contains("9950")
            || name.contains("9900")
            || name.contains("9700")
            || name.contains("9600")
            || name.contains("ZEN 5")
        {
            (32.0, 2, 1, 8, "IF 4.0", 64.0)
        } else if name.contains("7950") || name.contains("7900") {
            (32.0, 2, 1, 8, "IF 3.5", 36.0)
        } else if name.contains("7800") || name.contains("7700") || name.contains("7600") {
            (32.0, 1, 1, 8, "IF 3.5", 36.0)
        } else if name.contains("5950") || name.contains("5900") {
            (32.0, 2, 1, 8, "IF 3.0", 32.0)
        } else if name.contains("5800") || name.contains("5700") || name.contains("5600") {
            (32.0, 1, 1, 8, "IF 3.0", 32.0)
        } else if name.contains("EPYC 9") || name.contains("GENOA") || name.contains("TURIN") {
            (32.0, 12, 1, 8, "IF 4.0", 64.0)
        } else if name.contains("EPYC 7") {
            (16.0, 8, 1, 8, "IF 2.0", 32.0)
        } else if name.contains("THREADRIPPER 7") || name.contains("PRO 7") {
            (32.0, 8, 1, 8, "IF 3.5", 36.0)
        } else if name.contains("THREADRIPPER 5") || name.contains("PRO 5") {
            (32.0, 4, 1, 8, "IF 3.0", 32.0)
        } else {
            (16.0, 1, 1, 8, "IF", 16.0)
        };

        let mut links = Vec::new();

        // Inter-socket links for multi-socket
        if sockets > 1 {
            let inter_bw = if_speed_gts * 2.0 * 4.0; // 4 links typically
            for i in 0..sockets {
                for j in (i + 1)..sockets {
                    links.push(InterconnectLink {
                        link_type: InterconnectType::InfinityFabric,
                        source: format!("Socket {}", i),
                        destination: format!("Socket {}", j),
                        bandwidth_gbs: inter_bw,
                        bidirectional_bandwidth_gbs: inter_bw * 2.0,
                        width: 16,
                        speed_gts: if_speed_gts,
                        latency_ns: 120.0,
                        active: true,
                    });
                }
            }
        }

        // On-package CCD-to-IOD links
        if compute_dies > 1 {
            for i in 0..compute_dies {
                links.push(InterconnectLink {
                    link_type: InterconnectType::InfinityFabricOnPackage,
                    source: format!("CCD {}", i),
                    destination: "IOD".into(),
                    bandwidth_gbs: on_pkg_bw,
                    bidirectional_bandwidth_gbs: on_pkg_bw * 2.0,
                    width: 32,
                    speed_gts: if_speed_gts,
                    latency_ns: 40.0,
                    active: true,
                });
            }
        }

        let total_bw: f64 = links.iter().map(|l| l.bidirectional_bandwidth_gbs).sum();

        let chiplet = if compute_dies > 1 || io_dies > 0 {
            Some(ChipletTopology {
                compute_dies,
                io_dies,
                // The real division, where the core count is readable. The
                // table's figure is the generation's maximum CCD size, not this
                // part's.
                cores_per_die: physical_cores
                    .filter(|_| compute_dies > 0)
                    .map(|c| c / compute_dies),
                on_package_interconnect: InterconnectType::InfinityFabricOnPackage,
                on_package_bandwidth_gbs: on_pkg_bw,
            })
        } else {
            None
        };

        InterconnectTopology {
            sockets,
            links,
            chiplet_topology: chiplet,
            coherence_protocol: CoherenceProtocol::MOESI,
            total_bandwidth_gbs: total_bw,
            is_numa: sockets > 1 || compute_dies > 1,
            generation: gen.into(),
        }
    }

    fn infer_apple(name: &str) -> InterconnectTopology {
        let bw = if name.contains("M4 ULTRA") {
            800.0
        } else if name.contains("M4 MAX") || name.contains("M3 ULTRA") {
            546.0
        } else if name.contains("M4 PRO")
            || name.contains("M3 MAX")
            || name.contains("M3 PRO")
            || name.contains("M2 MAX")
        {
            400.0
        } else if name.contains("M2 PRO") || name.contains("M2") || name.contains("M1") {
            200.0
        } else {
            100.0
        };

        let links = vec![InterconnectLink {
            link_type: InterconnectType::AppleUMA,
            source: "CPU Cluster".into(),
            destination: "Unified Memory".into(),
            bandwidth_gbs: bw,
            bidirectional_bandwidth_gbs: bw,
            width: 0,
            speed_gts: 0.0,
            latency_ns: 20.0,
            active: true,
        }];

        InterconnectTopology {
            sockets: 1,
            links,
            chiplet_topology: if name.contains("ULTRA") {
                Some(ChipletTopology {
                    compute_dies: 2,
                    io_dies: 0,
                    // Apple's core counts vary by bin within a generation
                    // (an M4 Pro is 12 or 14), so this is not read either.
                    cores_per_die: None,
                    on_package_interconnect: InterconnectType::AppleUMA,
                    on_package_bandwidth_gbs: bw,
                })
            } else {
                None
            },
            coherence_protocol: CoherenceProtocol::Directory,
            total_bandwidth_gbs: bw,
            is_numa: false,
            generation: "UMA".into(),
        }
    }

    #[cfg(target_os = "linux")]
    fn get_cpu_model() -> Result<String, IronError> {
        let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").map_err(IronError::Io)?;
        for line in cpuinfo.lines() {
            if let Some(val) = line.strip_prefix("model name") {
                if let Some(name) = val.trim().strip_prefix(':') {
                    return Ok(name.trim().to_string());
                }
            }
        }
        Ok(String::new())
    }

    #[cfg(target_os = "linux")]
    fn detect_sockets() -> Result<u32, IronError> {
        // Count unique physical package IDs
        let mut sockets = std::collections::HashSet::new();
        let cpu_dir = std::path::Path::new("/sys/devices/system/cpu");

        if let Ok(entries) = std::fs::read_dir(cpu_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("cpu") && name[3..].chars().all(|c| c.is_ascii_digit()) {
                    let pkg_path = entry.path().join("topology/physical_package_id");
                    if let Ok(pkg) = std::fs::read_to_string(&pkg_path) {
                        sockets.insert(pkg.trim().to_string());
                    }
                }
            }
        }

        Ok(sockets.len().max(1) as u32)
    }

    #[cfg(target_os = "windows")]
    fn get_cpu_model() -> Result<String, IronError> {
        let output = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "(Get-CimInstance Win32_Processor).Name",
            ])
            .output()
            .map_err(IronError::Io)?;
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    #[cfg(target_os = "windows")]
    fn detect_sockets() -> Result<u32, IronError> {
        let output = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "(Get-CimInstance Win32_Processor | Measure-Object).Count",
            ])
            .output()
            .map_err(IronError::Io)?;
        let count = String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse::<u32>()
            .unwrap_or(1);
        Ok(count)
    }

    #[cfg(target_os = "macos")]
    fn get_cpu_model() -> Result<String, IronError> {
        let output = std::process::Command::new("sysctl")
            .args(["-n", "machdep.cpu.brand_string"])
            .output()
            .map_err(IronError::Io)?;
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    #[cfg(target_os = "macos")]
    fn detect_sockets() -> Result<u32, IronError> {
        Ok(1) // macOS is always single-socket
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    fn get_cpu_model() -> Result<String, IronError> {
        Ok(String::new())
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    fn detect_sockets() -> Result<u32, IronError> {
        Ok(1)
    }

    /// Physical cores in the package, or `None` where they are not readable.
    ///
    /// Used to divide by the compute-die count rather than take a table's
    /// maximum CCD size. Returns `None` rather than a guess, so
    /// `cores_per_die` is absent instead of wrong.
    #[cfg(target_os = "windows")]
    fn detect_physical_cores() -> Option<u32> {
        let output = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                concat!(
                    "(Get-CimInstance Win32_Processor | ",
                    "Measure-Object -Property NumberOfCores -Sum).Sum"
                ),
            ])
            .output()
            .ok()?;
        String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse::<u32>()
            .ok()
            .filter(|c| *c > 0)
    }

    /// See the Windows variant.
    #[cfg(target_os = "linux")]
    fn detect_physical_cores() -> Option<u32> {
        // Unique (physical id, core id) pairs in /proc/cpuinfo. Counting
        // "cpu cores" alone multiplies by the socket count.
        let text = std::fs::read_to_string("/proc/cpuinfo").ok()?;
        let mut seen = std::collections::HashSet::new();
        let (mut pkg, mut core) = (None, None);
        for line in text.lines() {
            if let Some(v) = line.strip_prefix("physical id") {
                pkg = v
                    .split(':')
                    .nth(1)
                    .and_then(|s| s.trim().parse::<u32>().ok());
            } else if let Some(v) = line.strip_prefix("core id") {
                core = v
                    .split(':')
                    .nth(1)
                    .and_then(|s| s.trim().parse::<u32>().ok());
            } else if line.trim().is_empty() {
                if let (Some(p), Some(c)) = (pkg.take(), core.take()) {
                    seen.insert((p, c));
                }
            }
        }
        if let (Some(p), Some(c)) = (pkg, core) {
            seen.insert((p, c));
        }
        (!seen.is_empty()).then_some(seen.len() as u32)
    }

    /// See the Windows variant.
    #[cfg(target_os = "macos")]
    fn detect_physical_cores() -> Option<u32> {
        let output = std::process::Command::new("sysctl")
            .args(["-n", "hw.physicalcpu"])
            .output()
            .ok()?;
        String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse::<u32>()
            .ok()
            .filter(|c| *c > 0)
    }

    /// See the Windows variant.
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    fn detect_physical_cores() -> Option<u32> {
        None
    }
}

impl Default for InterconnectMonitor {
    fn default() -> Self {
        Self::new().unwrap_or(Self {
            topology: InterconnectTopology {
                sockets: 1,
                links: Vec::new(),
                chiplet_topology: None,
                coherence_protocol: CoherenceProtocol::Unknown,
                total_bandwidth_gbs: 0.0,
                is_numa: false,
                generation: "Unknown".into(),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intel_upi_inference() {
        let topo = InterconnectMonitor::infer_intel("INTEL XEON GOLD 6348 SAPPHIRE RAPIDS", 2);
        assert!(topo.is_numa);
        assert_eq!(topo.sockets, 2);
        assert!(!topo.links.is_empty());
        // Should have UPI links
        assert!(topo
            .links
            .iter()
            .any(|l| l.link_type == InterconnectType::UPI));
    }

    #[test]
    fn test_amd_chiplet_inference() {
        // 16 physical cores over 2 CCDs is a 7950X: 8 per die.
        let topo = InterconnectMonitor::infer_amd("AMD RYZEN 9 7950X", 1, Some(16));
        assert!(topo.chiplet_topology.is_some());
        let chiplet = topo.chiplet_topology.unwrap();
        assert_eq!(chiplet.compute_dies, 2);
        assert_eq!(chiplet.cores_per_die, Some(8));
        assert!(chiplet.on_package_bandwidth_gbs > 0.0);
    }

    /// A partially-enabled part is not the generation's maximum CCD size.
    ///
    /// The table held 8 cores per die for all of Zen 4 and Zen 5, so a Ryzen 9
    /// 9900X — two CCDs of six — was reported as 2 x 8, and every
    /// partially-enabled part in the range with it. The division uses the
    /// physical core count now.
    #[test]
    fn a_partially_enabled_ccd_is_not_the_generations_maximum() {
        let topo = InterconnectMonitor::infer_amd("AMD RYZEN 9 9900X", 1, Some(12));
        let chiplet = topo.chiplet_topology.expect("a 9900X has chiplets");
        assert_eq!(chiplet.compute_dies, 2);
        assert_eq!(
            chiplet.cores_per_die,
            Some(6),
            "12 cores over 2 CCDs is 6 per die, not the table's 8"
        );
    }

    /// With no core count there is nothing to divide, so nothing is claimed.
    #[test]
    fn an_unreadable_core_count_yields_no_cores_per_die() {
        let topo = InterconnectMonitor::infer_amd("AMD RYZEN 9 9900X", 1, None);
        let chiplet = topo.chiplet_topology.expect("a 9900X has chiplets");
        assert_eq!(chiplet.cores_per_die, None);
    }

    /// AMD writes the core count into the model string, and the Intel test
    /// matched the word "Core" in it.
    ///
    /// "AMD Ryzen 9 9900X 12-Core Processor" uppercases to contain "CORE", so
    /// every Ryzen, EPYC and Threadripper was classified Intel and given a
    /// ring bus with MESIF coherence. This desktop reported exactly that.
    #[test]
    fn an_amd_core_count_does_not_make_it_an_intel_cpu() {
        for model in [
            "AMD Ryzen 9 9900X 12-Core Processor",
            "AMD Ryzen Threadripper PRO 7995WX 96-Cores",
            "AMD EPYC 9654 96-Core Processor",
        ] {
            let topo = InterconnectMonitor::infer_topology(model, 1, Some(12));
            assert_eq!(
                topo.coherence_protocol,
                CoherenceProtocol::MOESI,
                "{model} was classified as Intel"
            );
        }

        // The tokens that were meant to catch Intel still do.
        for model in [
            "Intel(R) Core(TM) i9-14900K",
            "Intel(R) Core(TM) Ultra 9 285K",
            "Intel(R) Xeon(R) Gold 6348",
        ] {
            let topo = InterconnectMonitor::infer_topology(model, 1, Some(8));
            assert_ne!(
                topo.coherence_protocol,
                CoherenceProtocol::MOESI,
                "{model} should not resolve as AMD"
            );
        }
    }

    #[test]
    fn test_apple_uma() {
        let topo = InterconnectMonitor::infer_apple("APPLE M4 PRO");
        assert!(!topo.is_numa);
        assert_eq!(topo.coherence_protocol, CoherenceProtocol::Directory);
        assert!(topo.total_bandwidth_gbs > 0.0);
    }

    #[test]
    fn test_interconnect_display() {
        assert_eq!(InterconnectType::UPI.to_string(), "Intel UPI");
        assert_eq!(
            InterconnectType::InfinityFabric.to_string(),
            "AMD Infinity Fabric"
        );
        assert_eq!(CoherenceProtocol::MOESI.to_string(), "MOESI");
    }

    #[test]
    fn test_monitor_default() {
        let monitor = InterconnectMonitor::default();
        let _topo = monitor.topology();
    }

    #[test]
    fn test_serialization() {
        let link = InterconnectLink {
            link_type: InterconnectType::UPI,
            source: "Socket 0".into(),
            destination: "Socket 1".into(),
            bandwidth_gbs: 64.0,
            bidirectional_bandwidth_gbs: 128.0,
            width: 3,
            speed_gts: 16.0,
            latency_ns: 60.0,
            active: true,
        };
        let json = serde_json::to_string(&link).unwrap();
        assert!(json.contains("Socket 0"));
        let _: InterconnectLink = serde_json::from_str(&json).unwrap();
    }
}
