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
    /// Unidirectional bandwidth in GB/s, or `None` where no figure is tabled
    /// for the link (the on-die link of every generation). A **specification**
    /// figure looked up by model name, never measured on this machine.
    pub bandwidth_gbs: Option<f64>,
    /// Bidirectional bandwidth in GB/s, or `None` as for `bandwidth_gbs`. A
    /// specification figure, as for `bandwidth_gbs`.
    pub bidirectional_bandwidth_gbs: Option<f64>,
    /// Link width (number of lanes/links), or `None` where none is tabled. A
    /// specification figure, as for `bandwidth_gbs`.
    pub width: Option<u32>,
    /// Speed per lane in GT/s (gigatransfers/second), or `None` where none is
    /// tabled. A specification figure, as for `bandwidth_gbs`.
    pub speed_gts: Option<f64>,
    /// Latency in nanoseconds: a **specification estimate** for the link type,
    /// a constant per kind of link, never measured on this machine. `None`
    /// where the link type has no tabled figure (Apple UMA).
    pub estimated_latency_ns: Option<f64>,
    /// Whether this link is active. Always `None`: no reader observes link
    /// state, and a model-name table cannot say whether a link is up. Used to
    /// report `true` for every link the table described.
    pub active: Option<bool>,
}

/// Chiplet/die topology for multi-die packages.
///
/// Present only where the CPU model matched an entry in the model table. The
/// die counts and bandwidth are specification values from that table, not
/// readings from this machine.
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
    /// On-package bandwidth per link in GB/s, or `None` where the die count is
    /// known but the table has no bandwidth for the model (Apple M1/M2 Ultra).
    pub on_package_bandwidth_gbs: Option<f64>,
}

/// Full interconnect topology report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterconnectTopology {
    /// Number of CPU sockets, or `None` where the package count was not read.
    pub sockets: Option<u32>,
    /// Inter-socket links.
    pub links: Vec<InterconnectLink>,
    /// Chiplet topology (if applicable).
    pub chiplet_topology: Option<ChipletTopology>,
    /// Coherence protocol in use, or `None` for a model the table does not list.
    /// Used to report MOESI, MESIF or Directory for every model.
    pub coherence_protocol: Option<CoherenceProtocol>,
    /// Total system interconnect bandwidth in GB/s: the bidirectional
    /// bandwidth of the links that carry a figure, or `None` where none does.
    /// A sum of specification figures, not a measurement.
    pub total_bandwidth_gbs: Option<f64>,
    /// Whether the system is NUMA, or `None` where the socket count is unknown
    /// and no die count settles it.
    pub is_numa: Option<bool>,
    /// Inferred interconnect generation/version.
    pub generation: String,
}

/// Sum of the links' bidirectional bandwidth in GB/s. A link with no figure is
/// left out of the sum; `None` when no link has one.
fn sum_bidirectional(links: &[InterconnectLink]) -> Option<f64> {
    links
        .iter()
        .filter_map(|l| l.bidirectional_bandwidth_gbs)
        .reduce(|a, b| a + b)
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

    /// Get inter-socket links only. Links are identified by their endpoints,
    /// since none has an observed active state.
    pub fn inter_socket_links(&self) -> Vec<&InterconnectLink> {
        self.topology
            .links
            .iter()
            .filter(|l| l.source.starts_with("Socket "))
            .collect()
    }

    /// Infer full topology from CPU model and socket count.
    fn infer_topology(
        cpu_model: &str,
        sockets: Option<u32>,
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
            Self::infer_apple(&upper, sockets)
        } else {
            InterconnectTopology {
                sockets,
                links: Vec::new(),
                chiplet_topology: None,
                // The vendor is not listed, so no protocol is claimed.
                coherence_protocol: None,
                total_bandwidth_gbs: None,
                is_numa: sockets.map(|s| s > 1),
                generation: "Unknown".into(),
            }
        }
    }

    fn infer_intel(name: &str, sockets: Option<u32>) -> InterconnectTopology {
        // Determine Intel interconnect generation. Speed and width are `None`
        // for the ring-bus and unlisted parts, which have no UPI link to
        // describe.
        let (link_type, speed_gts, width, gen, on_die, coherence): (
            InterconnectType,
            Option<f64>,
            Option<u32>,
            &str,
            InterconnectType,
            Option<CoherenceProtocol>,
        ) = if name.contains("GRANITE") || name.contains("EMERALD") || name.contains("SIERRA") {
            (
                InterconnectType::UPI,
                Some(20.0),
                Some(3),
                "UPI 2.0",
                InterconnectType::MeshInterconnect,
                Some(CoherenceProtocol::MESIF),
            )
        } else if name.contains("SAPPHIRE") || name.contains("W9-3") || name.contains("W7-3") {
            (
                InterconnectType::UPI,
                Some(16.0),
                Some(3),
                "UPI 1.1",
                InterconnectType::MeshInterconnect,
                Some(CoherenceProtocol::MESIF),
            )
        } else if name.contains("ICE LAKE") && name.contains("XEON") {
            (
                InterconnectType::UPI,
                Some(11.2),
                Some(3),
                "UPI 1.0",
                InterconnectType::MeshInterconnect,
                Some(CoherenceProtocol::MESIF),
            )
        } else if name.contains("XEON") && (name.contains("PLATINUM") || name.contains("GOLD")) {
            (
                InterconnectType::UPI,
                Some(10.4),
                Some(3),
                "UPI 1.0",
                InterconnectType::MeshInterconnect,
                Some(CoherenceProtocol::MESIF),
            )
        } else if name.contains("12TH")
            || name.contains("13TH")
            || name.contains("14TH")
            || name.contains("ALDER")
            || name.contains("RAPTOR")
        {
            (
                InterconnectType::RingBus,
                None,
                None,
                "Ring Bus",
                InterconnectType::RingBus,
                Some(CoherenceProtocol::MESIF),
            )
        } else if name.contains("ARROW") || name.contains("LUNAR") || name.contains("ULTRA") {
            (
                InterconnectType::RingBus,
                None,
                None,
                "Ring/Foveros",
                InterconnectType::RingBus,
                Some(CoherenceProtocol::MESIF),
            )
        } else {
            // An unlisted model gets no protocol. MESIF is Intel's, but the
            // table does not say which Intel parts use it.
            (
                InterconnectType::Unknown,
                None,
                None,
                "Unknown",
                InterconnectType::RingBus,
                None,
            )
        };

        // Bandwidth: speed (GT/s) * width (bytes) * links, where both are known.
        let uni_bw = speed_gts.zip(width).map(|(s, w)| s * w as f64 * 2.0); // 2 bytes per transfer on UPI
        let bidi_bw = uni_bw.map(|uni| uni * 2.0);

        let mut links = Vec::new();
        // Inter-socket links need a known socket count and a known speed.
        if let (Some(count), Some(speed)) = (sockets.filter(|&s| s > 1), speed_gts) {
            for i in 0..count {
                for j in (i + 1)..count {
                    links.push(InterconnectLink {
                        link_type,
                        source: format!("Socket {}", i),
                        destination: format!("Socket {}", j),
                        bandwidth_gbs: uni_bw,
                        bidirectional_bandwidth_gbs: bidi_bw,
                        width,
                        speed_gts: Some(speed),
                        estimated_latency_ns: Some(if speed >= 16.0 { 60.0 } else { 80.0 }),
                        active: None,
                    });
                }
            }
        }

        // On-die interconnect. No bandwidth, width or speed is tabled for it.
        links.push(InterconnectLink {
            link_type: on_die,
            source: "Die".into(),
            destination: "Cores/LLC".into(),
            bandwidth_gbs: None,
            bidirectional_bandwidth_gbs: None,
            width: None,
            speed_gts: None,
            estimated_latency_ns: Some(if matches!(on_die, InterconnectType::MeshInterconnect) {
                15.0
            } else {
                10.0
            }),
            active: None,
        });

        let total_bw = sum_bidirectional(&links);

        InterconnectTopology {
            sockets,
            links,
            chiplet_topology: None,
            coherence_protocol: coherence,
            total_bandwidth_gbs: total_bw,
            is_numa: sockets.map(|s| s > 1),
            generation: gen.into(),
        }
    }

    /// An AMD part the table does not describe: no chiplet topology and no
    /// link figures.
    fn unlisted_amd(sockets: Option<u32>) -> InterconnectTopology {
        InterconnectTopology {
            sockets,
            links: Vec::new(),
            chiplet_topology: None,
            // Nothing is described for this model, so there is no total.
            total_bandwidth_gbs: None,
            // The table does not describe this model, so no protocol is claimed
            // for it. Used to report MOESI, which is AMD's for the listed parts.
            coherence_protocol: None,
            is_numa: sockets.map(|s| s > 1),
            generation: "Unknown".into(),
        }
    }

    fn infer_amd(
        name: &str,
        sockets: Option<u32>,
        physical_cores: Option<u32>,
    ) -> InterconnectTopology {
        // AMD Infinity Fabric inference.
        //
        // APUs (a model number ending in G or GE, such as 5600G) are one
        // monolithic die, not the chiplet parts that share their numbers, so
        // they take the unlisted path below.
        let apu = name.split_whitespace().any(|token| {
            let digits = token.trim_end_matches("GE").trim_end_matches('G');
            digits.len() < token.len()
                && digits.len() >= 4
                && digits.chars().all(|c| c.is_ascii_digit())
        });
        let (if_speed_gts, compute_dies, io_dies, _max_cores_per_die, gen, on_pkg_bw) = if apu {
            return Self::unlisted_amd(sockets);
        } else if name.contains("9950") || name.contains("9900") {
            (32.0, 2, 1, 8, "IF 4.0", 64.0)
        } else if name.contains("9700") || name.contains("9600") {
            // One CCD: the 6- and 8-core Zen 5 desktop parts. The table used to
            // give two, which doubled their CCD count.
            (32.0, 1, 1, 8, "IF 4.0", 64.0)
        } else if name.contains("7950") || name.contains("7900") {
            (32.0, 2, 1, 8, "IF 3.5", 36.0)
        } else if name.contains("7800") || name.contains("7700") || name.contains("7600") {
            (32.0, 1, 1, 8, "IF 3.5", 36.0)
        } else if name.contains("5950") || name.contains("5900") {
            (32.0, 2, 1, 8, "IF 3.0", 32.0)
        } else if name.contains("5800") || name.contains("5700") || name.contains("5600") {
            (32.0, 1, 1, 8, "IF 3.0", 32.0)
        // EPYC and Threadripper PRO/non-PRO are not given a die count here. The
        // CCD count varies by SKU within a family (a 7960X has four CCDs where
        // the family's maximum is eight), and no SKU table is held, so they take
        // the unlisted path below rather than a guessed count.
        } else {
            // A model the table does not list gets no chiplet topology and no
            // link figures. The table's default row is not a description of any
            // part, so it is not applied to one.
            return Self::unlisted_amd(sockets);
        };

        let mut links = Vec::new();

        // Inter-socket links for multi-socket, where the socket count is known.
        // The link count and width are not known from the model name, so no
        // bandwidth or width is given. The table used to assume four links of
        // sixteen lanes, which was a guess.
        if let Some(count) = sockets.filter(|&s| s > 1) {
            for i in 0..count {
                for j in (i + 1)..count {
                    links.push(InterconnectLink {
                        link_type: InterconnectType::InfinityFabric,
                        source: format!("Socket {}", i),
                        destination: format!("Socket {}", j),
                        bandwidth_gbs: None,
                        bidirectional_bandwidth_gbs: None,
                        width: None,
                        speed_gts: Some(if_speed_gts),
                        estimated_latency_ns: Some(120.0),
                        active: None,
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
                    bandwidth_gbs: Some(on_pkg_bw),
                    bidirectional_bandwidth_gbs: Some(on_pkg_bw * 2.0),
                    width: Some(32),
                    speed_gts: Some(if_speed_gts),
                    estimated_latency_ns: Some(40.0),
                    active: None,
                });
            }
        }

        let total_bw = sum_bidirectional(&links);

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
                on_package_bandwidth_gbs: Some(on_pkg_bw),
            })
        } else {
            None
        };

        InterconnectTopology {
            sockets,
            links,
            chiplet_topology: chiplet,
            coherence_protocol: Some(CoherenceProtocol::MOESI),
            total_bandwidth_gbs: total_bw,
            // Two compute dies are NUMA whatever the socket count; otherwise the
            // socket count decides, and is unknown where it was not read.
            is_numa: if compute_dies > 1 {
                Some(true)
            } else {
                sockets.map(|s| s > 1)
            },
            generation: gen.into(),
        }
    }

    fn infer_apple(name: &str, sockets: Option<u32>) -> InterconnectTopology {
        // Apple's published unified-memory bandwidth, GB/s. Variants are
        // matched before base chips, because "M2 ULTRA" also contains "M2".
        // The table read 200 GB/s for a base M1 (68.25) and M2 (100), and
        // 400 for an M4 Pro (273). M3 Max and M4 Max ship at two bandwidths
        // depending on the bin, which the name does not say, so they have no
        // entry; neither does any chip not listed.
        const TABLE: &[(&str, f64)] = &[
            ("M1 ULTRA", 800.0),
            ("M2 ULTRA", 800.0),
            ("M3 ULTRA", 819.0),
            ("M1 MAX", 400.0),
            ("M2 MAX", 400.0),
            ("M1 PRO", 200.0),
            ("M2 PRO", 200.0),
            ("M3 PRO", 150.0),
            ("M4 PRO", 273.0),
            ("M1", 68.25),
            ("M2", 100.0),
            ("M3", 100.0),
            ("M4", 120.0),
        ];
        let variant = ["ULTRA", "MAX", "PRO"]
            .into_iter()
            .find(|v| name.contains(v));
        let entry = TABLE.iter().find(|(chip, _)| {
            // A base-chip entry only matches a name with no variant.
            name.contains(chip) && (chip.contains(' ') || variant.is_none())
        });
        let bw: Option<f64> = entry.map(|&(_, bw)| bw);

        // Without a bandwidth there is no link to describe, so none is built
        // rather than one carrying a guessed figure.
        let links = match bw {
            Some(bw) => vec![InterconnectLink {
                link_type: InterconnectType::AppleUMA,
                source: "CPU Cluster".into(),
                destination: "Unified Memory".into(),
                bandwidth_gbs: Some(bw),
                bidirectional_bandwidth_gbs: Some(bw),
                // Width and speed are neither read nor tabled for this fabric.
                width: None,
                speed_gts: None,
                // No latency is tabled for Apple UMA, so none is given.
                estimated_latency_ns: None,
                active: None,
            }],
            None => Vec::new(),
        };
        let total_bw = sum_bidirectional(&links);

        InterconnectTopology {
            // The package count is the one read from the platform, not assumed.
            sockets,
            links,
            chiplet_topology: if name.contains("ULTRA") {
                Some(ChipletTopology {
                    // Ultra is two dies fused by UltraFusion, so this count is
                    // fixed by the design rather than read.
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
            // Directory coherence is stated for the listed chips only. An
            // unlisted chip gets no protocol, as an unlisted AMD part does.
            coherence_protocol: entry.map(|_| CoherenceProtocol::Directory),
            total_bandwidth_gbs: total_bw,
            is_numa: sockets.map(|s| s > 1),
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
    fn detect_sockets() -> Result<Option<u32>, IronError> {
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

        // No readable package ID means the count is unknown, not one socket.
        Ok((!sockets.is_empty()).then_some(sockets.len() as u32))
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
    fn detect_sockets() -> Result<Option<u32>, IronError> {
        let output = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "(Get-CimInstance Win32_Processor | Measure-Object).Count",
            ])
            .output()
            .map_err(IronError::Io)?;
        // Output that is not a positive count is unknown, not one socket.
        Ok(String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse::<u32>()
            .ok()
            .filter(|c| *c > 0))
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
    fn detect_sockets() -> Result<Option<u32>, IronError> {
        // The package count the kernel reports, not an assumed one.
        let output = std::process::Command::new("sysctl")
            .args(["-n", "hw.packages"])
            .output()
            .map_err(IronError::Io)?;
        Ok(String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse::<u32>()
            .ok()
            .filter(|c| *c > 0))
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    fn get_cpu_model() -> Result<String, IronError> {
        Ok(String::new())
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    fn detect_sockets() -> Result<Option<u32>, IronError> {
        Ok(None)
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
        // The same count `cpu_microarch` reports, so the two cannot disagree.
        let text = std::fs::read_to_string("/proc/cpuinfo").ok()?;
        crate::cpu_microarch::physical_cores_from_cpuinfo(&text)
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
        // Used only when the read fails: nothing was read, so nothing is
        // claimed. Not one socket, and not a zero-bandwidth interconnect.
        Self::new().unwrap_or(Self {
            topology: InterconnectTopology {
                sockets: None,
                links: Vec::new(),
                chiplet_topology: None,
                coherence_protocol: None,
                total_bandwidth_gbs: None,
                is_numa: None,
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
        let topo =
            InterconnectMonitor::infer_intel("INTEL XEON GOLD 6348 SAPPHIRE RAPIDS", Some(2));
        assert_eq!(topo.is_numa, Some(true));
        assert_eq!(topo.sockets, Some(2));
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
        let topo = InterconnectMonitor::infer_amd("AMD RYZEN 9 7950X", Some(1), Some(16));
        assert!(topo.chiplet_topology.is_some());
        let chiplet = topo.chiplet_topology.unwrap();
        assert_eq!(chiplet.compute_dies, 2);
        assert_eq!(chiplet.cores_per_die, Some(8));
        assert_eq!(chiplet.on_package_bandwidth_gbs, Some(36.0));
    }

    /// A partially-enabled part is not the generation's maximum CCD size.
    ///
    /// The table held 8 cores per die for all of Zen 4 and Zen 5, so a Ryzen 9
    /// 9900X — two CCDs of six — was reported as 2 x 8, and every
    /// partially-enabled part in the range with it. The division uses the
    /// physical core count now.
    #[test]
    fn a_partially_enabled_ccd_is_not_the_generations_maximum() {
        let topo = InterconnectMonitor::infer_amd("AMD RYZEN 9 9900X", Some(1), Some(12));
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
        let topo = InterconnectMonitor::infer_amd("AMD RYZEN 9 9900X", Some(1), None);
        let chiplet = topo.chiplet_topology.expect("a 9900X has chiplets");
        assert_eq!(chiplet.cores_per_die, None);
    }

    /// A model the table does not list has no chiplet topology.
    ///
    /// The table's default row used to apply to any AMD name it did not match,
    /// so a Ryzen 5 8600G was reported with one CCD, one IOD and 16 GB/s.
    #[test]
    fn an_unlisted_amd_model_has_no_chiplet_topology() {
        let topo = InterconnectMonitor::infer_amd("AMD RYZEN 5 8600G", Some(1), Some(6));
        assert!(topo.chiplet_topology.is_none());
        assert!(topo.links.is_empty());
        assert_eq!(topo.generation, "Unknown");
        assert_eq!(topo.total_bandwidth_gbs, None);

        // Substrings of other names: "RYZEN 5" contains "ZEN 5", and
        // "RYZEN 7 PRO 7840U" contains "PRO 7" (a Threadripper PRO row).
        for name in ["AMD RYZEN 7 PRO 7840U", "AMD RYZEN 5 PRO 5650U"] {
            let topo = InterconnectMonitor::infer_amd(name, Some(1), Some(8));
            assert!(topo.chiplet_topology.is_none(), "{name}");
        }
        // An APU is one die, whatever chiplet part shares its number.
        let topo = InterconnectMonitor::infer_amd("AMD RYZEN 7 5700G", Some(1), Some(8));
        assert!(topo.chiplet_topology.is_none());
        let topo = InterconnectMonitor::infer_amd("AMD RYZEN 7 5700X", Some(1), Some(8));
        assert!(topo.chiplet_topology.is_some());
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
            let topo = InterconnectMonitor::infer_topology(model, Some(1), Some(12));
            // EPYC and Threadripper are unlisted (no SKU die table), so no
            // protocol is claimed for them. What must not happen is the Intel
            // MESIF classification.
            assert_ne!(
                topo.coherence_protocol,
                Some(CoherenceProtocol::MESIF),
                "{model} was classified as Intel"
            );
        }

        // The tokens that were meant to catch Intel still do.
        for model in [
            "Intel(R) Core(TM) i9-14900K",
            "Intel(R) Core(TM) Ultra 9 285K",
            "Intel(R) Xeon(R) Gold 6348",
        ] {
            let topo = InterconnectMonitor::infer_topology(model, Some(1), Some(8));
            assert_ne!(
                topo.coherence_protocol,
                Some(CoherenceProtocol::MOESI),
                "{model} should not resolve as AMD"
            );
        }
    }

    #[test]
    fn test_apple_uma() {
        let topo = InterconnectMonitor::infer_apple("APPLE M4 PRO", Some(1));
        assert_eq!(topo.is_numa, Some(false));
        assert_eq!(topo.coherence_protocol, Some(CoherenceProtocol::Directory));
        assert_eq!(topo.total_bandwidth_gbs, Some(273.0));
    }

    /// "M2 ULTRA" contains "M2", which the base-chip arm used to match first,
    /// so an M2 Ultra was given the M2's figure. Variants match their own
    /// entry, and a base chip's entry matches only a name with no variant.
    #[test]
    fn an_apple_variant_does_not_take_its_base_chips_bandwidth() {
        let topo = InterconnectMonitor::infer_apple("APPLE M2 ULTRA", Some(1));
        let chiplet = topo.chiplet_topology.expect("an Ultra is two dies");
        assert_eq!(chiplet.compute_dies, 2);
        assert_eq!(chiplet.on_package_bandwidth_gbs, Some(800.0));
        let bw = |name| {
            InterconnectMonitor::infer_apple(name, Some(1))
                .links
                .first()
                .and_then(|l| l.bandwidth_gbs)
        };
        assert_eq!(bw("APPLE M1"), Some(68.25));
        assert_eq!(bw("APPLE M4 PRO"), Some(273.0));
        // Binned at two bandwidths that the name does not distinguish.
        assert_eq!(bw("APPLE M4 MAX"), None);
        assert_eq!(bw("APPLE M3 MAX"), None);
        assert_eq!(bw("APPLE M4 ULTRA"), None);
    }

    /// A name the table does not list has no bandwidth and no topology, rather
    /// than the 100 GB/s the fallback used to report.
    #[test]
    fn an_unlisted_apple_model_has_no_bandwidth() {
        let topo = InterconnectMonitor::infer_apple("APPLE M9", Some(1));
        assert!(topo.links.is_empty());
        assert!(topo.chiplet_topology.is_none());
        assert_eq!(topo.coherence_protocol, None);
    }

    /// The socket count is the one that was read. None stays None, rather than
    /// the single package Apple silicon is assumed to have.
    #[test]
    fn an_apple_socket_count_is_read_not_assumed() {
        let unread = InterconnectMonitor::infer_apple("APPLE M4 PRO", None);
        assert_eq!(unread.sockets, None);
        assert_eq!(unread.is_numa, None);
        let read = InterconnectMonitor::infer_apple("APPLE M4 PRO", Some(1));
        assert_eq!(read.sockets, Some(1));
    }

    /// No reader observes link state, so no link claims to be active.
    #[test]
    fn no_link_claims_to_be_active() {
        let topo =
            InterconnectMonitor::infer_intel("INTEL XEON GOLD 6348 SAPPHIRE RAPIDS", Some(2));
        assert!(!topo.links.is_empty());
        assert!(topo.links.iter().all(|l| l.active.is_none()));
        let monitor = InterconnectMonitor { topology: topo };
        // The inter-socket link is found by its endpoints, not by an active flag.
        assert_eq!(monitor.inter_socket_links().len(), 1);
    }

    /// An unlisted AMD model has no protocol to report.
    #[test]
    fn an_unlisted_amd_model_has_no_coherence_protocol() {
        let topo = InterconnectMonitor::infer_amd("AMD RYZEN 5 8600G", Some(1), Some(6));
        assert_eq!(topo.coherence_protocol, None);
        let listed = InterconnectMonitor::infer_amd("AMD RYZEN 9 7950X", Some(1), Some(16));
        assert_eq!(listed.coherence_protocol, Some(CoherenceProtocol::MOESI));
    }

    /// A part with no link figures has no total, rather than a total of zero.
    #[test]
    fn a_part_with_no_links_has_no_total_bandwidth() {
        let unlisted = InterconnectMonitor::infer_amd("AMD RYZEN 5 8600G", Some(1), Some(6));
        assert_eq!(unlisted.total_bandwidth_gbs, None);

        // A single-die 5700X describes no inter-socket or on-package link.
        let single_die = InterconnectMonitor::infer_amd("AMD RYZEN 7 5700X", Some(1), Some(8));
        assert!(single_die.links.is_empty());
        assert_eq!(single_die.total_bandwidth_gbs, None);
    }

    /// A ring bus has no UPI speed or width, so its on-die link carries no
    /// bandwidth and the total is absent.
    #[test]
    fn a_ring_bus_has_no_link_figures() {
        let topo = InterconnectMonitor::infer_intel("INTEL CORE I9 RAPTOR LAKE", Some(1));
        assert_eq!(topo.links.len(), 1);
        let on_die = &topo.links[0];
        assert_eq!(on_die.bandwidth_gbs, None);
        assert_eq!(on_die.width, None);
        assert_eq!(on_die.speed_gts, None);
        assert_eq!(topo.total_bandwidth_gbs, None);
        assert_eq!(topo.is_numa, Some(false));
    }

    /// An unknown socket count is unknown, not one socket. NUMA is settled
    /// only where the die count settles it.
    #[test]
    fn an_unknown_socket_count_leaves_numa_unknown() {
        let intel = InterconnectMonitor::infer_intel("INTEL XEON GOLD 6348 SAPPHIRE RAPIDS", None);
        assert_eq!(intel.sockets, None);
        assert_eq!(intel.is_numa, None);
        // No inter-socket link can be built without a count.
        assert!(intel.links.iter().all(|l| l.source == "Die"));

        let single_ccd = InterconnectMonitor::infer_amd("AMD RYZEN 7 5700X", None, Some(8));
        assert_eq!(single_ccd.is_numa, None);

        // Two compute dies are NUMA whatever the socket count.
        let two_ccd = InterconnectMonitor::infer_amd("AMD RYZEN 9 9900X", None, Some(12));
        assert_eq!(two_ccd.is_numa, Some(true));

        let unknown = InterconnectMonitor::infer_topology("SOME UNKNOWN CPU", None, None);
        assert_eq!(unknown.sockets, None);
        assert_eq!(unknown.is_numa, None);
        assert_eq!(unknown.total_bandwidth_gbs, None);
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
            bandwidth_gbs: Some(64.0),
            bidirectional_bandwidth_gbs: Some(128.0),
            width: Some(3),
            speed_gts: Some(16.0),
            estimated_latency_ns: Some(60.0),
            active: None,
        };
        let json = serde_json::to_string(&link).unwrap();
        assert!(json.contains("Socket 0"));
        let _: InterconnectLink = serde_json::from_str(&json).unwrap();
    }
}
