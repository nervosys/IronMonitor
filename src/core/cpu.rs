//! CPU monitoring

use serde::{Deserialize, Serialize};

/// CPU frequency information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuFrequency {
    /// Current frequency in MHz, or `None` where it was not read.
    ///
    /// These three were `u32` and a zero meant "not measured" — a sentinel
    /// introduced by the fix that stopped Windows reporting the nominal clock
    /// as the current one, on the reasoning that the ontology resolver already
    /// read zero that way. It does. Nothing else did: `ironmon cli cpu` printed
    /// "Clock: 0 MHz", and the agent surface published
    /// `"frequency_mhz": 0` from four call sites.
    ///
    /// A CPU does not run at zero hertz, so the sentinel was at least
    /// implausible rather than plausible — but it was still a number where
    /// there was no reading.
    pub current: Option<u32>,
    /// Whether [`Self::current`] was measured or computed.
    ///
    /// Linux reads `scaling_cur_freq`, which is a clock. Windows has no
    /// unprivileged API that reports one -- `CallNtPowerInformation` returns
    /// the nominal figure for every core -- so it multiplies the nominal
    /// maximum by `% Processor Performance`, which the kernel derives from
    /// APERF and MPERF. That product is a real per-core number and it is not a
    /// direct reading, and the resolver publishes it as `Derived` on the
    /// strength of this flag rather than letting one entity's provenance mean
    /// two different things on two platforms.
    pub current_is_derived: bool,
    /// Minimum frequency in MHz, or `None`. No Win32 API reports one.
    pub min: Option<u32>,
    /// Maximum frequency in MHz, or `None` where it was not read.
    pub max: Option<u32>,
}

/// Per-core CPU information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuCore {
    /// Core ID
    pub id: usize,
    /// Online status, or `None` where it was not read. Windows and macOS have
    /// no reader for it, and the Linux reader leaves it `None` when a core's
    /// `online` file is missing. These used to be a literal `true`, which
    /// reported every core online whether or not anything had checked.
    pub online: Option<bool>,
    /// Governor type, or `None` where the kernel's governor was not read. A driver
    /// name is not a governor, so no reader fills this with one.
    pub governor: Option<String>,
    /// Frequency information
    pub frequency: Option<CpuFrequency>,
    /// User mode utilization percentage
    pub user: Option<f32>,
    /// Nice mode utilization percentage
    pub nice: Option<f32>,
    /// System mode utilization percentage
    pub system: Option<f32>,
    /// Idle percentage
    pub idle: Option<f32>,
    /// CPU model name, or `None` where the platform did not report one. This
    /// was `"Unknown"` or an empty string on the failure path, which is a name
    /// some reader made up rather than a reading.
    pub model: Option<String>,
}

/// Aggregate CPU totals
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuTotal {
    /// Total user mode percentage
    pub user: f32,
    /// Total nice mode percentage, or `None` where the source has no nice time.
    /// Windows has no nice concept, and `top` does not separate it.
    pub nice: Option<f32>,
    /// Total system mode percentage
    pub system: f32,
    /// Total idle percentage
    pub idle: f32,
}

/// CPU statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuStats {
    /// Per-core information
    pub cores: Vec<CpuCore>,
    /// Aggregate totals
    pub total: CpuTotal,
}

impl CpuStats {
    /// A zeroed struct for a platform reader to fill in.
    ///
    /// **This reads nothing.** It returns no cores and 100% idle, and it was called `new()`
    /// until 6.0.0 — a name that reads like a constructor that gathers
    /// data. Two GUI defects, one in the HTTP server and one in the
    /// library's own `snapshot_cpu` came from exactly that misreading,
    /// found over three separate occasions.
    ///
    /// The real values come from the per-platform readers. Use those
    /// unless you are a reader building your own starting struct.
    pub fn empty() -> Self {
        Self {
            cores: Vec::new(),
            total: CpuTotal {
                user: 0.0,
                nice: None,
                system: 0.0,
                idle: 100.0,
            },
        }
    }

    /// Get number of CPU cores
    pub fn core_count(&self) -> usize {
        self.cores.len()
    }

    /// Number of online cores, or `None` when any core's online state was not
    /// read. A count that left out unread cores would be short, and reporting
    /// zero online on Windows, where no reader fills the flag, was wrong.
    pub fn online_count(&self) -> Option<usize> {
        if self.cores.iter().any(|c| c.online.is_none()) {
            return None;
        }
        Some(self.cores.iter().filter(|c| c.online == Some(true)).count())
    }
}

impl Default for CpuStats {
    fn default() -> Self {
        Self::empty()
    }
}
