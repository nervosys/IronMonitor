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
    /// Online status
    pub online: bool,
    /// Governor type
    pub governor: String,
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
    /// CPU model name
    pub model: String,
}

/// Aggregate CPU totals
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuTotal {
    /// Total user mode percentage
    pub user: f32,
    /// Total nice mode percentage
    pub nice: f32,
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
                nice: 0.0,
                system: 0.0,
                idle: 100.0,
            },
        }
    }

    /// Get number of CPU cores
    pub fn core_count(&self) -> usize {
        self.cores.len()
    }

    /// Get number of online cores
    pub fn online_count(&self) -> usize {
        self.cores.iter().filter(|c| c.online).count()
    }
}

impl Default for CpuStats {
    fn default() -> Self {
        Self::empty()
    }
}
