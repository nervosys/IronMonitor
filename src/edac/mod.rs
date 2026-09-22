//! Error Detection and Correction (EDAC) monitoring.
//!
//! Reports ECC memory errors by memory controller and DIMM, including
//! correctable (CE) and uncorrectable (UE) counts, DIMM labels, locations,
//! and grain sizes.
//!
//! ## Platform Support
//!
//! - **Linux**: `/sys/devices/system/edac/mc*/`, when an EDAC driver is loaded
//! - **Windows**: nothing. This line used to read "`wmic memorychip` for ECC
//!   support detection", and no code here has ever run `wmic` or read anything
//!   else on Windows -- `scan()` returned an empty overview, which the ontology
//!   published as "the platform interface exists but enumerated nothing".
//!   Whether the *modules* carry ECC is a different question and is read, from
//!   SMBIOS, for `memory.dimm.{n}.ecc`.
//! - **macOS**: nothing, for the same reason

use crate::error::IronError;
use serde::{Deserialize, Serialize};

/// EDAC memory type (from EDAC subsystem).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdacMemType {
    Ddr,
    Ddr2,
    Ddr3,
    Ddr4,
    Ddr5,
    Rddr3,
    Rddr4,
    Rddr5,
    Lpddr4,
    Lpddr5,
    Hbm2,
    Hbm3,
    Unknown,
}

impl std::fmt::Display for EdacMemType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ddr => write!(f, "DDR"),
            Self::Ddr2 => write!(f, "DDR2"),
            Self::Ddr3 => write!(f, "DDR3"),
            Self::Ddr4 => write!(f, "DDR4"),
            Self::Ddr5 => write!(f, "DDR5"),
            Self::Rddr3 => write!(f, "Registered DDR3"),
            Self::Rddr4 => write!(f, "Registered DDR4"),
            Self::Rddr5 => write!(f, "Registered DDR5"),
            Self::Lpddr4 => write!(f, "LPDDR4"),
            Self::Lpddr5 => write!(f, "LPDDR5"),
            Self::Hbm2 => write!(f, "HBM2"),
            Self::Hbm3 => write!(f, "HBM3"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

/// EDAC DIMM/CSROW information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdacCsRow {
    /// CSROW index.
    pub index: u32,
    /// DIMM label.
    pub label: String,
    /// Memory type.
    pub mem_type: EdacMemType,
    /// Size in MB, or `None` where sysfs did not report it.
    pub size_mb: Option<u64>,
    /// Correctable errors count, or `None` where the counter was not readable.
    ///
    /// **Not the same as zero.** A counter that could not be read and a DIMM
    /// that has recorded no errors are opposite facts, and the second one tells
    /// an operator their memory is healthy.
    pub ce_count: Option<u64>,
    /// Uncorrectable errors count, or `None` where unreadable. See
    /// [`Self::ce_count`].
    pub ue_count: Option<u64>,
    /// Location (channel/slot).
    pub location: String,
    /// Grain size (error resolution in bytes), or `None` where unreadable.
    pub grain: Option<u32>,
}

/// EDAC memory controller.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdacMemoryController {
    /// MC index.
    pub index: u32,
    /// MC name/driver.
    pub mc_name: String,
    /// Total correctable errors on this controller, or `None` where the
    /// counter was not readable. See [`EdacCsRow::ce_count`].
    pub ce_count: Option<u64>,
    /// Total uncorrectable errors on this controller, or `None` where
    /// unreadable.
    pub ue_count: Option<u64>,
    /// Unattributed correctable errors, or `None` where unreadable.
    pub ce_noinfo_count: Option<u64>,
    /// Unattributed uncorrectable errors, or `None` where unreadable.
    pub ue_noinfo_count: Option<u64>,
    /// CSROW / DIMM entries.
    pub csrows: Vec<EdacCsRow>,
    /// Seconds since reset, or `None` where unreadable.
    pub seconds_since_reset: Option<u64>,
}

/// EDAC overview.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdacOverview {
    /// Memory controllers.
    pub controllers: Vec<EdacMemoryController>,
    /// Total MC count.
    pub total_controllers: u32,
    /// Total correctable errors across all controllers.
    ///
    /// `None` when any controller's counter was unreadable: a total computed
    /// over partly-unknown inputs is not a total, and this one is the number an
    /// operator reads as "is my memory failing".
    pub total_ce: Option<u64>,
    /// Total uncorrectable errors across all controllers. `None` on the same
    /// terms as [`Self::total_ce`].
    pub total_ue: Option<u64>,
    /// Whether ECC is active.
    pub ecc_active: bool,
    /// Recommendations.
    pub recommendations: Vec<String>,
}

/// EDAC monitor.
pub struct EdacMonitor {
    overview: EdacOverview,
}

impl EdacMonitor {
    /// Create a new EDAC monitor.
    pub fn new() -> Result<Self, IronError> {
        let overview = Self::scan()?;
        Ok(Self { overview })
    }

    /// Refresh.
    pub fn refresh(&mut self) -> Result<(), IronError> {
        self.overview = Self::scan()?;
        Ok(())
    }

    /// Get overview.
    pub fn overview(&self) -> &EdacOverview {
        &self.overview
    }

    /// Get controllers.
    pub fn controllers(&self) -> &[EdacMemoryController] {
        &self.overview.controllers
    }

    /// Total CE count, or `None` when any controller's counter was unreadable.
    pub fn total_correctable_errors(&self) -> Option<u64> {
        self.overview.total_ce
    }

    /// Total UE count, or `None` on the same terms as
    /// [`Self::total_correctable_errors`].
    pub fn total_uncorrectable_errors(&self) -> Option<u64> {
        self.overview.total_ue
    }

    /// All DIMMs with errors.
    pub fn dimms_with_errors(&self) -> Vec<&EdacCsRow> {
        self.overview
            .controllers
            .iter()
            .flat_map(|mc| mc.csrows.iter())
            // A DIMM whose counters were unreadable is not a DIMM with
            // errors; it is a DIMM nobody could ask. It stays out of this list
            // rather than being counted either way.
            .filter(|cs| cs.ce_count.is_some_and(|c| c > 0) || cs.ue_count.is_some_and(|c| c > 0))
            .collect()
    }

    #[cfg(target_os = "linux")]
    fn scan() -> Result<EdacOverview, IronError> {
        // An absent `edac` directory is the driver not being loaded, which is
        // not the same fact as a loaded driver finding no controller -- and an
        // empty overview said the second while meaning the first. The
        // distinction is the whole difference between "this machine reports no
        // ECC errors" and "nothing here counts them".
        let mc_base = std::path::Path::new("/sys/devices/system/edac");
        if !mc_base.exists() {
            return Err(IronError::FeatureNotAvailable(
                "the kernel exposes no /sys/devices/system/edac: no EDAC driver \
                 is loaded for this memory controller"
                    .into(),
            ));
        }

        let mut controllers = Vec::new();

        // Scan mc0, mc1, ... directories
        for i in 0..16 {
            let mc_path = mc_base.join(format!("mc/mc{}", i));
            if !mc_path.exists() {
                continue;
            }

            let mc_name =
                Self::read_sysfs(&mc_path.join("mc_name")).unwrap_or_else(|| format!("mc{}", i));
            // `read_sysfs_u64` already answers `None` for a file that is
            // missing or unreadable. These used to discard that and report `0`,
            // which is an ECC error count an operator acts on.
            let ce_count = Self::read_sysfs_u64(&mc_path.join("ce_count"));
            let ue_count = Self::read_sysfs_u64(&mc_path.join("ue_count"));
            let ce_noinfo = Self::read_sysfs_u64(&mc_path.join("ce_noinfo_count"));
            let ue_noinfo = Self::read_sysfs_u64(&mc_path.join("ue_noinfo_count"));
            let seconds = Self::read_sysfs_u64(&mc_path.join("seconds_since_reset"));

            let mut csrows = Vec::new();

            // Scan csrow0, csrow1, ...
            for j in 0..32 {
                let csrow_path = mc_path.join(format!("csrow{}", j));
                if !csrow_path.exists() {
                    continue;
                }

                let label = Self::read_sysfs(&csrow_path.join("ch0_dimm_label"))
                    .or_else(|| Self::read_sysfs(&csrow_path.join("dimm_label")))
                    .unwrap_or_else(|| format!("csrow{}", j));

                let mem_type_str =
                    Self::read_sysfs(&csrow_path.join("mem_type")).unwrap_or_default();
                let mem_type = Self::parse_mem_type(&mem_type_str);

                let size_mb = Self::read_sysfs_u64(&csrow_path.join("size_mb"));
                let cs_ce = Self::read_sysfs_u64(&csrow_path.join("ce_count"));
                let cs_ue = Self::read_sysfs_u64(&csrow_path.join("ue_count"));
                let grain = Self::read_sysfs_u32(&csrow_path.join("grain"));

                let location = Self::read_sysfs(&csrow_path.join("location")).unwrap_or_default();

                csrows.push(EdacCsRow {
                    index: j,
                    label,
                    mem_type,
                    size_mb,
                    ce_count: cs_ce,
                    ue_count: cs_ue,
                    location,
                    grain,
                });
            }

            // Also scan dimmN directories (newer EDAC layout)
            for j in 0..64 {
                let dimm_path = mc_path.join(format!("dimm{}", j));
                if !dimm_path.exists() {
                    continue;
                }

                let label = Self::read_sysfs(&dimm_path.join("dimm_label"))
                    .unwrap_or_else(|| format!("dimm{}", j));

                let mem_type_str =
                    Self::read_sysfs(&dimm_path.join("dimm_mem_type")).unwrap_or_default();
                let mem_type = Self::parse_mem_type(&mem_type_str);

                let size_mb = Self::read_sysfs_u64(&dimm_path.join("size"));
                let cs_ce = Self::read_sysfs_u64(&dimm_path.join("dimm_ce_count"));
                let cs_ue = Self::read_sysfs_u64(&dimm_path.join("dimm_ue_count"));

                let location =
                    Self::read_sysfs(&dimm_path.join("dimm_location")).unwrap_or_default();

                csrows.push(EdacCsRow {
                    index: j,
                    label,
                    mem_type,
                    size_mb,
                    ce_count: cs_ce,
                    ue_count: cs_ue,
                    location,
                    // The dimm layout exposes no grain attribute. `0` claimed an
                    // error resolution of zero bytes, which is not a value this
                    // interface can report.
                    grain: None,
                });
            }

            controllers.push(EdacMemoryController {
                index: i,
                mc_name,
                ce_count,
                ue_count,
                ce_noinfo_count: ce_noinfo,
                ue_noinfo_count: ue_noinfo,
                csrows,
                seconds_since_reset: seconds,
            });
        }

        let total = controllers.len() as u32;
        // Summed only when every contributing counter was read. One unreadable
        // controller used to reduce the fleet-visible error count silently,
        // which is the failure mode a total exists to avoid.
        let total_ce: Option<u64> = controllers
            .iter()
            .map(|mc| mc.ce_count)
            .try_fold(0u64, |acc, c| c.map(|c| acc + c));
        let total_ue: Option<u64> = controllers
            .iter()
            .map(|mc| mc.ue_count)
            .try_fold(0u64, |acc, c| c.map(|c| acc + c));
        let ecc_active = total > 0;

        let mut recs = Vec::new();

        // **An unreadable counter earns a recommendation of its own.** Silence
        // here used to mean "no errors", because an unreadable counter arrived
        // as `0` and simply failed every threshold. An operator reading an empty
        // recommendation list concluded the memory was fine. Now the absence
        // says so, which is the one thing it could never say before.
        match total_ue {
            Some(count) if count > 0 => recs.push(format!(
                "CRITICAL: {} uncorrectable ECC error(s) detected — replace affected DIMM(s)",
                count
            )),
            Some(_) => {}
            None => recs.push(
                "ECC uncorrectable-error counters could not be read on at least one                  controller — memory health cannot be assessed from this host"
                    .into(),
            ),
        }
        match total_ce {
            Some(count) if count > 100 => recs.push(format!(
                "WARNING: {} correctable ECC errors — monitor for increasing rate",
                count
            )),
            Some(_) => {}
            None => recs.push(
                "ECC correctable-error counters could not be read on at least one                  controller — a rising error rate would not be visible here"
                    .into(),
            ),
        }
        if !ecc_active {
            recs.push(
                "No EDAC memory controllers found — ECC may be disabled or not supported".into(),
            );
        }

        Ok(EdacOverview {
            controllers,
            total_controllers: total,
            total_ce,
            total_ue,
            ecc_active,
            recommendations: recs,
        })
    }

    #[cfg(target_os = "linux")]
    fn parse_mem_type(s: &str) -> EdacMemType {
        match s.to_lowercase().as_str() {
            "ddr" => EdacMemType::Ddr,
            "ddr2" => EdacMemType::Ddr2,
            "ddr3" => EdacMemType::Ddr3,
            "ddr4" | "unbuffered-ddr4" => EdacMemType::Ddr4,
            "ddr5" | "unbuffered-ddr5" => EdacMemType::Ddr5,
            "rddr3" | "registered-ddr3" => EdacMemType::Rddr3,
            "rddr4" | "registered-ddr4" => EdacMemType::Rddr4,
            "rddr5" | "registered-ddr5" => EdacMemType::Rddr5,
            "lpddr4" => EdacMemType::Lpddr4,
            "lpddr5" => EdacMemType::Lpddr5,
            _ => EdacMemType::Unknown,
        }
    }

    #[cfg(target_os = "linux")]
    fn read_sysfs(path: &std::path::Path) -> Option<String> {
        std::fs::read_to_string(path)
            .ok()
            .map(|s| s.trim().to_string())
    }

    #[cfg(target_os = "linux")]
    fn read_sysfs_u64(path: &std::path::Path) -> Option<u64> {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| s.trim().parse().ok())
    }

    #[cfg(target_os = "linux")]
    fn read_sysfs_u32(path: &std::path::Path) -> Option<u32> {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| s.trim().parse().ok())
    }

    /// There is no EDAC equivalent this crate reads off Linux, and saying so
    /// is not the same as reporting zero controllers.
    ///
    /// This returned an empty overview, which the ontology resolver read as
    /// "the platform interface exists but enumerated nothing" and published as
    /// the reason three ECC entities were absent. Nothing had been asked: the
    /// function consulted no interface on any platform but Linux. The
    /// resolver's other branch -- the one whose comment already said "on
    /// Windows and macOS there is no EDAC equivalent IronMonitor reads, so this is
    /// the common path" -- was unreachable, and the comment described what the
    /// author believed rather than what the code did.
    ///
    /// Windows does report the array's error-correction type in SMBIOS, as
    /// `Win32_PhysicalMemoryArray.MemoryErrorCorrection` (3, "None", on the
    /// development host). That answers whether the modules carry ECC, which is
    /// the per-slot `memory.dimm.{n}.ecc` entity and is already read; it does
    /// not answer whether a controller is *reporting* corrections, which is
    /// what these three entities ask and what no Windows interface exposes.
    #[cfg(not(target_os = "linux"))]
    fn scan() -> Result<EdacOverview, IronError> {
        Err(IronError::UnsupportedPlatform(
            "ECC error counts are read from /sys/devices/system/edac, which \
             exists on Linux only; this platform exposes no interface ironmon \
             reads for correctable and uncorrectable counts"
                .into(),
        ))
    }

    fn empty_overview() -> EdacOverview {
        EdacOverview {
            controllers: Vec::new(),
            total_controllers: 0,
            // Nothing was read, so there is no total. `0` here claimed a clean
            // bill of health for a machine nobody examined.
            total_ce: None,
            total_ue: None,
            ecc_active: false,
            recommendations: Vec::new(),
        }
    }
}

impl Default for EdacMonitor {
    fn default() -> Self {
        Self::new().unwrap_or(Self {
            overview: Self::empty_overview(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mem_type_display() {
        assert_eq!(EdacMemType::Ddr4.to_string(), "DDR4");
        assert_eq!(EdacMemType::Ddr5.to_string(), "DDR5");
        assert_eq!(EdacMemType::Rddr4.to_string(), "Registered DDR4");
    }

    #[test]
    fn test_dimms_with_errors() {
        let overview = EdacOverview {
            controllers: vec![EdacMemoryController {
                index: 0,
                mc_name: "test_mc".into(),
                ce_count: Some(5),
                ue_count: Some(0),
                ce_noinfo_count: Some(0),
                ue_noinfo_count: Some(0),
                csrows: vec![
                    EdacCsRow {
                        index: 0,
                        label: "DIMM_A1".into(),
                        mem_type: EdacMemType::Ddr4,
                        size_mb: Some(16384),
                        ce_count: Some(5),
                        ue_count: Some(0),
                        location: "ch0/slot0".into(),
                        grain: Some(8),
                    },
                    EdacCsRow {
                        index: 1,
                        label: "DIMM_A2".into(),
                        mem_type: EdacMemType::Ddr4,
                        size_mb: Some(16384),
                        ce_count: Some(0),
                        ue_count: Some(0),
                        location: "ch0/slot1".into(),
                        grain: Some(8),
                    },
                ],
                seconds_since_reset: Some(86400),
            }],
            total_controllers: 1,
            total_ce: Some(5),
            total_ue: Some(0),
            ecc_active: true,
            recommendations: Vec::new(),
        };
        let monitor = EdacMonitor { overview };
        let errored = monitor.dimms_with_errors();
        assert_eq!(errored.len(), 1);
        assert_eq!(errored[0].label, "DIMM_A1");
    }

    #[test]
    fn test_monitor_default() {
        let monitor = EdacMonitor::default();
        let _overview = monitor.overview();
    }

    #[test]
    fn test_serialization() {
        let cs = EdacCsRow {
            index: 0,
            label: "DIMM0".into(),
            mem_type: EdacMemType::Ddr5,
            size_mb: Some(32768),
            ce_count: Some(3),
            ue_count: Some(0),
            location: "mc0/ch0/dimm0".into(),
            grain: Some(8),
        };
        let json = serde_json::to_string(&cs).unwrap();
        assert!(json.contains("DIMM0"));
        let _: EdacCsRow = serde_json::from_str(&json).unwrap();
    }
}
