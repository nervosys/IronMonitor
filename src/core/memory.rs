//! Memory monitoring

use serde::{Deserialize, Serialize};

/// RAM information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RamInfo {
    /// Total RAM in KB
    pub total: u64,
    /// Used RAM in KB
    pub used: u64,
    /// Free RAM in KB: memory holding nothing at all, not counting cache the
    /// kernel would reclaim on demand. `None` where it was not read.
    ///
    /// Linux reports `MemFree` and macOS `Pages free`. Windows reported
    /// `ullAvailPhys`, which is *available* -- the standby list included -- so
    /// on the machine this was found on "free" was 60.9 GB where 31.7 GB was.
    /// It now reads `\Memory\Free & Zero Page List Bytes`, and a failed read
    /// is `None` rather than the available figure under this name.
    pub free: Option<u64>,
    /// Buffered RAM in KB, or `None` where the platform has no such figure.
    ///
    /// "Buffers" is a Linux `/proc/meminfo` notion. Windows and macOS have no
    /// equivalent, and both reported `0` -- a measurement of no buffered
    /// memory, on systems where the quantity does not exist.
    pub buffers: Option<u64>,
    /// Cached RAM in KB, or `None` where it was not read.
    ///
    /// Windows does have this: `GetPerformanceInfo` reports `SystemCache` in
    /// pages, which is what Task Manager's "Cached" figure comes from. It was
    /// `0` under a comment reading "Could use GetPerformanceInfo for
    /// SystemCache" -- an admission that the number was not measured, sitting
    /// two lines above `shared: None`, which had already been fixed for exactly
    /// this reason.
    pub cached: Option<u64>,
    /// Shared RAM in KB (GPU shared on Jetson). `None` when the platform does
    /// not report it — which is most of them, and was a fabricated zero until
    /// 6.0.0.
    pub shared: Option<u64>,
    /// Large Free Blocks (4MB blocks on Jetson)
    pub lfb: Option<u32>,
}

/// SWAP information.
///
/// Every field is `Option` because zero and unknown are different facts and a
/// `u64` cannot hold both. `Some(0)` means the platform reported no swap;
/// `None` means it did not report.
///
/// This mattered concretely. The ontology resolver reads `total == 0` as "no
/// swap or pagefile configured", a definite claim about the machine, and until
/// 6.0.0 a reader that could not determine swap had no way to say so: the Linux
/// parser filled zeros for a missing `/proc/meminfo` field, and the macOS reader
/// had to fail the entire memory read — losing the RAM figures — rather than
/// pass a zero through. The type is the fix; the workarounds were the symptom.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SwapInfo {
    /// Total SWAP in KB. `None` when the platform did not report it.
    pub total: Option<u64>,
    /// Used SWAP in KB. `None` when the platform did not report it.
    pub used: Option<u64>,
    /// Cached SWAP in KB. `None` when the platform does not expose it — Windows
    /// has no equivalent figure at all.
    pub cached: Option<u64>,
}

impl SwapInfo {
    /// Total KB, treating "not reported" as zero.
    ///
    /// For display and serialisation where a number is required. A caller that
    /// needs to distinguish no-swap from unknown must match on the field; this
    /// exists so that choosing not to is visible at the call site.
    pub fn total_or_zero(&self) -> u64 {
        self.total.unwrap_or(0)
    }

    /// Used KB, treating "not reported" as zero. See [`Self::total_or_zero`].
    pub fn used_or_zero(&self) -> u64 {
        self.used.unwrap_or(0)
    }

    /// Cached KB, treating "not reported" as zero. See [`Self::total_or_zero`].
    pub fn cached_or_zero(&self) -> u64 {
        self.cached.unwrap_or(0)
    }

    /// Whether the platform reported swap at all.
    pub fn is_reported(&self) -> bool {
        self.total.is_some()
    }
}

/// EMC (External Memory Controller) information (Jetson only)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmcInfo {
    /// EMC online status
    pub online: bool,
    /// Percentage of bandwidth used
    pub value: u32,
    /// Current frequency in kHz
    pub current: u32,
    /// Maximum frequency in kHz
    pub max: u32,
    /// Minimum frequency in kHz
    pub min: u32,
}

/// IRAM information (Jetson only)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IramInfo {
    /// Total IRAM in KB
    pub total: u64,
    /// Used IRAM in KB
    pub used: u64,
    /// Large Free Blocks
    pub lfb: Option<u32>,
}

/// Memory statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    /// RAM information
    pub ram: RamInfo,
    /// SWAP information
    pub swap: SwapInfo,
    /// EMC information (Jetson only)
    pub emc: Option<EmcInfo>,
    /// IRAM information (Jetson only)
    pub iram: Option<IramInfo>,
}

impl MemoryStats {
    /// A zeroed struct for a platform reader to fill in.
    ///
    /// **This reads nothing.** It returns all zeros, and it was called `new()`
    /// until 6.0.0 — a name that reads like a constructor that gathers
    /// data. Two GUI defects, one in the HTTP server and one in the
    /// library's own `snapshot_memory` came from exactly that misreading,
    /// found over three separate occasions.
    ///
    /// The real values come from the per-platform readers. Use those
    /// unless you are a reader building your own starting struct.
    pub fn empty() -> Self {
        Self {
            ram: RamInfo {
                total: 0,
                used: 0,
                free: None,
                buffers: None,
                cached: None,
                shared: None,
                lfb: None,
            },
            // `None`, not `Some(0)`: an empty struct has read nothing, and
            // claiming a machine has no swap is a claim.
            swap: SwapInfo {
                total: None,
                used: None,
                cached: None,
            },
            emc: None,
            iram: None,
        }
    }

    /// Get RAM usage percentage
    pub fn ram_usage_percent(&self) -> f32 {
        if self.ram.total == 0 {
            0.0
        } else {
            (self.ram.used as f32 / self.ram.total as f32) * 100.0
        }
    }

    /// SWAP usage percentage, or `None` when swap was not reported.
    ///
    /// Returns `Some(0.0)` for a machine with no swap, which is a reading, and
    /// `None` when the platform did not say — a caller that renders those the
    /// same way is choosing to, rather than being unable to tell.
    pub fn swap_usage_percent(&self) -> Option<f32> {
        let total = self.swap.total?;
        let used = self.swap.used?;
        if total == 0 {
            Some(0.0)
        } else {
            Some((used as f32 / total as f32) * 100.0)
        }
    }

    /// SWAP usage percentage for display, treating "not reported" as zero.
    ///
    /// Named so the call site says it is making that choice. Every caller that
    /// used to get this silently now has to write it down.
    pub fn swap_usage_percent_or_zero(&self) -> f32 {
        self.swap_usage_percent().unwrap_or(0.0)
    }
}

impl Default for MemoryStats {
    fn default() -> Self {
        Self::empty()
    }
}
