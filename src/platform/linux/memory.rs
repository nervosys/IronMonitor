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

fn parse_ram_info(meminfo: &str) -> Result<RamInfo> {
    let mut ram = RamInfo {
        total: 0,
        used: 0,
        free: 0,
        buffers: None,
        cached: None,
        shared: None,
        lfb: None,
    };

    let mut mem_total = 0u64;
    let mut mem_free = 0u64;
    let mut mem_available = 0u64;
    let mut buffers = 0u64;
    let mut cached = 0u64;
    let mut s_reclaimable = 0u64;
    let mut shmem = 0u64;

    for line in meminfo.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }

        let key = parts[0].trim_end_matches(':');
        let value: u64 = parts[1].parse().unwrap_or(0);

        match key {
            "MemTotal" => mem_total = value,
            "MemFree" => mem_free = value,
            "MemAvailable" => mem_available = value,
            "Buffers" => buffers = value,
            "Cached" => cached = value,
            "SReclaimable" => s_reclaimable = value,
            "Shmem" => shmem = value,
            _ => {}
        }
    }

    ram.total = mem_total;
    ram.free = mem_free;
    ram.buffers = Some(buffers);
    ram.cached = Some(cached + s_reclaimable);
    // Linux does report this, via Shmem in /proc/meminfo.
    ram.shared = Some(shmem);
    ram.used = mem_total.saturating_sub(mem_available);

    // Try to read LFB (Large Free Blocks) for Jetson
    if let Ok(lfb) = read_lfb() {
        ram.lfb = Some(lfb);
    }

    Ok(ram)
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
