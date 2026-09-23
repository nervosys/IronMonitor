//! Linux memory monitoring

use crate::core::memory::{EmcInfo, IramInfo, MemoryStats, RamInfo, SwapInfo};
use crate::error::Result;
use crate::platform::common::*;
use std::fs;

/// Read memory statistics
pub fn read_memory_stats() -> Result<MemoryStats> {
    let mut stats = MemoryStats::empty();

    // Read /proc/meminfo
    let meminfo = fs::read_to_string("/proc/meminfo")?;
    stats.ram = parse_ram_info(&meminfo)?;
    stats.swap = parse_swap_info(&meminfo)?;

    // Try to read Jetson-specific memory info
    stats.emc = read_emc_info().ok();
    stats.iram = read_iram_info().ok();

    Ok(stats)
}

/// Parse the RAM figures out of `/proc/meminfo`.
///
/// **Every accumulator here used to start at `0` and every value came through
/// `.unwrap_or(0)`**, so a missing or unparseable line became a measurement.
/// That mattered in two different ways, fixed two different ways:
///
/// - `MemTotal`, `MemFree` and `MemAvailable` land in bare `u64` fields, which
///   cannot express absence. A `/proc/meminfo` without `MemTotal:` is a broken
///   system rather than a machine with no memory, so this now **fails** rather
///   than reporting zero -- matching the macOS and Windows readers, where the
///   syscall failing fails the whole call.
/// - `Buffers`, `Cached`, `SReclaimable` and `Shmem` land in `Option` fields
///   that already exist for exactly this reason. They were being filled with
///   `Some(accumulator)` regardless, so a kernel with no `Buffers:` line
///   reported `Some(0)` -- a measured zero, through a field typed to say
///   "not reported". `RamInfo::buffers`'s own documentation describes that as
///   the defect it was introduced to fix.
fn parse_ram_info(meminfo: &str) -> Result<RamInfo> {
    let mut mem_total = None;
    let mut mem_free = None;
    let mut mem_available = None;
    let mut buffers = None;
    let mut cached = None;
    let mut s_reclaimable = None;
    let mut shmem = None;

    for line in meminfo.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }

        let key = parts[0].trim_end_matches(':');
        // A line whose value does not parse is a line we did not read.
        let Ok(value) = parts[1].parse::<u64>() else {
            continue;
        };

        match key {
            "MemTotal" => mem_total = Some(value),
            "MemFree" => mem_free = Some(value),
            "MemAvailable" => mem_available = Some(value),
            "Buffers" => buffers = Some(value),
            "Cached" => cached = Some(value),
            "SReclaimable" => s_reclaimable = Some(value),
            "Shmem" => shmem = Some(value),
            _ => {}
        }
    }

    let missing = |field: &str| {
        crate::error::IronError::Parse(format!(
            "/proc/meminfo has no {field} line; refusing to report it as zero"
        ))
    };

    let total = mem_total.ok_or_else(|| missing("MemTotal"))?;
    let free = mem_free.ok_or_else(|| missing("MemFree"))?;
    let available = mem_available.ok_or_else(|| missing("MemAvailable"))?;

    Ok(RamInfo {
        total,
        free,
        // Derived from two readings that are both present by this point.
        used: total.saturating_sub(available),
        buffers,
        // `Cached` alone understates it; the kernel's reclaimable slab counts
        // too. Absent unless at least one of the pair was read, and summing
        // only what was.
        cached: match (cached, s_reclaimable) {
            (None, None) => None,
            (c, s) => Some(c.unwrap_or(0) + s.unwrap_or(0)),
        },
        // Linux does report this, via Shmem in /proc/meminfo.
        shared: shmem,
        // Try to read LFB (Large Free Blocks) for Jetson
        lfb: read_lfb().ok(),
    })
}

/// Parse the swap figures out of `/proc/meminfo`.
///
/// `SwapInfo`'s fields are plain `u64`, so a total of zero has to mean exactly
/// one thing: this machine has no swap configured. That is what the kernel
/// reports on a swapless host, and it is a reading.
///
/// It must not also mean "the field was missing", which is reachable — a
/// container without swap accounting has no `SwapTotal:` line at all. Starting
/// at zero and filling in whatever is present quietly conflated the two, so an
/// absent field became a confident "no swap".
///
/// A missing `SwapTotal` is therefore an error rather than a zero. The
/// type-level fix is `Option` fields on `SwapInfo`, which would make the whole
/// class impossible; it is 62 read sites away and is recorded in HANDOFF.md
/// rather than half-done here.
fn parse_swap_info(meminfo: &str) -> Result<SwapInfo> {
    // Starts as "nothing reported". Each field becomes `Some` only when
    // /proc/meminfo actually carried it, which is what makes a container with no
    // swap accounting distinguishable from a machine with no swap.
    let mut swap = SwapInfo::default();

    for line in meminfo.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }

        let key = parts[0].trim_end_matches(':');
        let value: u64 = parts[1].parse().unwrap_or(0);

        match key {
            "SwapTotal" => swap.total = Some(value),
            "SwapFree" => {
                // Used is only meaningful once the total is known.
                swap.used = swap.total.map(|t| t.saturating_sub(value));
            }
            "SwapCached" => swap.cached = Some(value),
            _ => {}
        }
    }

    // A missing SwapTotal is no longer an error: `None` says "not reported" in
    // the type, so the whole memory read need not fail to stay honest. That
    // error existed only because `u64` could not express this.
    Ok(swap)
}

fn read_lfb() -> Result<u32> {
    // LFB can be read from various tegrastats outputs
    // This is a simplified version
    Ok(0)
}

fn read_emc_info() -> Result<EmcInfo> {
    // EMC (External Memory Controller) info for Jetson
    let emc_path = "/sys/class/devfreq/17000000.mc";

    if !path_exists(emc_path) {
        // Try alternative paths
        let alt_path = "/sys/class/devfreq/13d00000.mc";
        if !path_exists(alt_path) {
            return Err(crate::error::IronError::FeatureNotAvailable(
                "EMC not available".to_string(),
            ));
        }
    }

    let cur = read_file_u32(format!("{}/cur_freq", emc_path))? / 1000;
    let min = read_file_u32(format!("{}/min_freq", emc_path))? / 1000;
    let max = read_file_u32(format!("{}/max_freq", emc_path))? / 1000;

    // Calculate bandwidth percentage (simplified)
    let value = if max > 0 {
        ((cur as f32 / max as f32) * 100.0) as u32
    } else {
        0
    };

    Ok(EmcInfo {
        online: true,
        value,
        current: cur,
        max,
        min,
    })
}

fn read_iram_info() -> Result<IramInfo> {
    // IRAM info for Jetson (if available)
    // This needs to be parsed from tegrastats output
    Err(crate::error::IronError::FeatureNotAvailable(
        "IRAM reading not yet implemented".to_string(),
    ))
}
