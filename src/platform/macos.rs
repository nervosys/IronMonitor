// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024 NervoSys

//! macOS CPU and memory readers.
//!
//! # Why this shells out
//!
//! The precise way to read these is `sysctlbyname`, `host_statistics64` and
//! `host_processor_info` through `libc`. This module instead parses the output of
//! `sysctl`, `vm_stat` and `top`, which the rest of the crate's macOS code already
//! does (see `cpu_cache`), for one reason that outweighs elegance here: **this code
//! has never run on a Mac.** It was written and compile-checked by
//! cross-compilation from Windows.
//!
//! Mach FFI that has never executed is a poor bet — a wrong structure offset or a
//! misread unit produces numbers that look plausible and are wrong, which is the
//! exact failure this project has spent two releases removing. Parsing documented
//! textual output can at least be tested: every parser below is a pure function
//! over a byte string, and the tests feed them real captured output. Those tests
//! run on Linux, Windows and macOS alike.
//!
//! # The one exception
//!
//! [`per_core_ticks`] does call Mach, through `libc::host_processor_info`. It is
//! the only source of per-core figures and of nice time, both of which `top`
//! cannot give at all — so unlike the rest, there is no textual alternative to
//! weigh it against. The two objections above are also answered for it
//! specifically: it is a `libc` call whose signature the compiler checks rather
//! than a structure whose offsets are hand-written, and `tests/macos_readers.rs`
//! executes it on `macos-latest` on every push, asserting that each core's split
//! accounts for that core and that cores differ from one another.
//!
//! `top` parsing remains as the fallback if that call fails, which is the only
//! path on which nice time is still reported as 0.0.

use std::process::Command;
#[cfg(target_os = "macos")]
use std::sync::Mutex;
use std::time::Duration;

/// Aggregate CPU time split, percent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CpuUsage {
    pub user: f32,
    pub system: f32,
    pub idle: f32,
}

/// Parse the last `CPU usage:` line of `top -l 2 -s 1 -n 0`.
///
/// The line reads, on every macOS version this targets:
/// `CPU usage: 4.76% user, 9.52% sys, 85.71% idle`
///
/// `top` prints one such line per sample. The first is the average since boot, so
/// the last one, which covers the interval, is the reading.
///
/// Returns `None` when the line is absent or malformed, so that a change in
/// `top`'s output surfaces as missing data rather than as zeros.
pub fn parse_top_cpu(output: &str) -> Option<CpuUsage> {
    let line = output
        .lines()
        .rfind(|l| l.trim_start().starts_with("CPU usage"))?;
    let rest = line.split_once(':')?.1;

    let field = |label: &str| -> Option<f32> {
        rest.split(',')
            .find(|part| part.trim_end().ends_with(label))?
            .trim()
            .split('%')
            .next()?
            .trim()
            .parse()
            .ok()
    };

    let user = field("user")?;
    let system = field("sys")?;
    let idle = field("idle")?;

    // A split that does not account for the whole CPU means the line was
    // understood incorrectly, not that the machine is idle.
    if !(95.0..=105.0).contains(&(user + system + idle)) {
        return None;
    }

    Some(CpuUsage { user, system, idle })
}

/// Page counts from `vm_stat`, in pages.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VmStat {
    pub page_size: u64,
    pub free: u64,
    pub active: u64,
    pub inactive: u64,
    pub speculative: u64,
    pub wired: u64,
    pub compressed: u64,
    pub file_backed: u64,
}

impl VmStat {
    /// Memory macOS counts as in use.
    ///
    /// Activity Monitor's "Memory Used" is app memory plus wired plus compressed.
    /// Inactive and file-backed pages are cache the kernel will surrender on
    /// demand, so counting them as used overstates pressure — the number a user
    /// compares against is the one Activity Monitor shows.
    pub fn used_bytes(&self) -> u64 {
        (self.active + self.wired + self.compressed) * self.page_size
    }

    pub fn free_bytes(&self) -> u64 {
        (self.free + self.speculative) * self.page_size
    }

    /// File-backed pages, which is what "cached" means on this platform.
    pub fn cached_bytes(&self) -> u64 {
        self.file_backed * self.page_size
    }
}

/// Parse `vm_stat` output.
///
/// The header carries the page size — it is 16384 on Apple Silicon and 4096 on
/// Intel, so it must be read rather than assumed:
/// `Mach Virtual Memory Statistics: (page size of 16384 bytes)`
///
/// Counts are printed with a trailing period: `Pages free:  123456.`
pub fn parse_vm_stat(output: &str) -> Option<VmStat> {
    let page_size = output
        .lines()
        .next()
        .and_then(|l| l.split("page size of ").nth(1))
        .and_then(|s| s.split_whitespace().next())
        .and_then(|s| s.parse::<u64>().ok())?;

    // Every line is required. A missing one used to parse as `0`, so free,
    // used and cached memory could each be understated by a whole category
    // while the struct looked complete. `/proc/meminfo` on Linux is read the
    // same way: a missing line fails the read rather than reading as zero.
    let value = |label: &str| -> Option<u64> {
        output
            .lines()
            .find(|l| l.trim_start().starts_with(label))
            .and_then(|l| l.split_once(':'))
            .map(|(_, v)| v.trim().trim_end_matches('.').replace(',', ""))
            .and_then(|v| v.parse().ok())
    };

    Some(VmStat {
        page_size,
        free: value("Pages free")?,
        active: value("Pages active")?,
        inactive: value("Pages inactive")?,
        speculative: value("Pages speculative")?,
        wired: value("Pages wired down")?,
        // Named "Pages occupied by compressor" on current macOS.
        compressed: value("Pages occupied by compressor")?,
        file_backed: value("File-backed pages")?,
    })
}

/// Swap totals in bytes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SwapUsage {
    pub total: u64,
    pub used: u64,
}

/// Parse `sysctl -n vm.swapusage`.
///
/// `total = 2048.00M  used = 1024.50M  free = 1023.50M  (encrypted)`
///
/// The suffix varies with size — M and G both occur — so it is read rather than
/// assumed, and a machine with swap disabled reports zeros, which is a reading.
pub fn parse_swapusage(output: &str) -> Option<SwapUsage> {
    let field = |label: &str| -> Option<u64> {
        let after = output.split(&format!("{label} = ")).nth(1)?;
        let token = after.split_whitespace().next()?;
        let (number, suffix) = token.split_at(token.find(|c: char| c.is_alphabetic())?);
        let value: f64 = number.parse().ok()?;
        let scale = match suffix {
            "K" | "k" => 1024.0,
            "M" | "m" => 1024.0 * 1024.0,
            "G" | "g" => 1024.0 * 1024.0 * 1024.0,
            "T" | "t" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
            "B" | "b" | "" => 1.0,
            _ => return None,
        };
        Some((value * scale) as u64)
    };

    Some(SwapUsage {
        total: field("total")?,
        used: field("used")?,
    })
}

/// Cumulative CPU ticks for one core, as the kernel counts them since boot.
///
/// A single reading is an average over uptime, not a current split. The split is the
/// difference between two readings, see [`usage_between`] and [`current_total_usage`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CoreTicks {
    pub user: u64,
    pub system: u64,
    pub idle: u64,
    pub nice: u64,
}

/// A core's time split as percentages.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CoreUsage {
    pub user: f32,
    pub nice: f32,
    pub system: f32,
    pub idle: f32,
}

impl CoreTicks {
    /// Convert to percentages, or `None` if the core has accumulated no time.
    ///
    /// Zero total means the counters were not read — a core that has genuinely run
    /// for zero ticks does not exist on a machine that is executing this code. It
    /// must not become "0% used, 0% idle", which reads as a measurement.
    pub fn percentages(&self) -> Option<CoreUsage> {
        let total = self.user + self.system + self.idle + self.nice;
        if total == 0 {
            return None;
        }
        let pct = |v: u64| (v as f64 / total as f64 * 100.0) as f32;
        Some(CoreUsage {
            user: pct(self.user),
            nice: pct(self.nice),
            system: pct(self.system),
            idle: pct(self.idle),
        })
    }
}

/// Sum per-core ticks into one aggregate.
pub fn aggregate_ticks(cores: &[CoreTicks]) -> CoreTicks {
    cores.iter().fold(CoreTicks::default(), |mut acc, c| {
        // Saturating because these are u64 counters summed across up to 128 cores;
        // wrapping would turn a busy machine into an idle one.
        acc.user = acc.user.saturating_add(c.user);
        acc.system = acc.system.saturating_add(c.system);
        acc.idle = acc.idle.saturating_add(c.idle);
        acc.nice = acc.nice.saturating_add(c.nice);
        acc
    })
}

/// How long to wait between the two readings that make a current split.
///
/// The kernel counts CPU ticks at about 100 per second, so 200 ms accumulates roughly
/// twenty ticks per core: enough for the split to mean something, and short enough
/// that the first call of a one-shot command does not stall on it.
pub const PRIME_INTERVAL: Duration = Duration::from_millis(200);

/// The per-core readings the last split was taken from.
///
/// Process-wide, because a split is a difference between two readings and no caller
/// should have to hold the earlier one itself. Both the platform reader and the
/// `stats` reader go through [`current_total_usage`], so they share this one value.
/// The whole-CPU reading is the sum of these, so keeping the per-core vector keeps both
/// figures on the same interval.
#[cfg(target_os = "macos")]
static PREVIOUS_CORES: Mutex<Option<Vec<CoreTicks>>> = Mutex::new(None);

/// The per-core splits between two readings, one entry per core.
///
/// Each core is differenced against its own earlier reading, through [`usage_between`].
/// A core whose counters went backwards or did not move is `None`. A change in the core
/// count leaves every entry `None`: the cores cannot be paired, so none is a split.
pub fn per_core_usage_between(
    earlier: &[CoreTicks],
    later: &[CoreTicks],
) -> Vec<Option<CoreUsage>> {
    if earlier.len() != later.len() {
        return vec![None; later.len()];
    }
    earlier
        .iter()
        .zip(later)
        .map(|(before, now)| usage_between(before, now))
        .collect()
}

/// The whole-CPU split between two aggregate readings, or `None` if there is none.
///
/// `None` in two cases, and neither is replaced by a guess:
/// - a counter went backwards, so the difference carries no information (the
///   counters were reset, or the readings are not from the same boot);
/// - nothing advanced between the readings, so the split is undefined.
///
/// The later reading alone is never a fallback. On its own it is the average since
/// boot, which is what this function exists to keep out of "current load".
pub fn usage_between(earlier: &CoreTicks, later: &CoreTicks) -> Option<CoreUsage> {
    let delta = CoreTicks {
        user: later.user.checked_sub(earlier.user)?,
        system: later.system.checked_sub(earlier.system)?,
        idle: later.idle.checked_sub(earlier.idle)?,
        nice: later.nice.checked_sub(earlier.nice)?,
    };
    delta.percentages()
}

/// The whole-CPU split and the per-core splits over the window since the previous call.
///
/// The per-core vector has one entry per core. It is `None` for a core that did not
/// move, and entirely `None` if the core count changed since the earlier reading.
///
/// Before this existed the first call in a process differenced nothing and reported
/// the average since boot. Every one-shot command (`ironmon get`, `ironmon snapshot`)
/// is a first call, so each of them presented an uptime average as current load.
///
/// With no earlier reading in the process, this takes one, waits [`PRIME_INTERVAL`]
/// and reads again. If the counters did not move between two reads, which happens
/// when they fall inside one tick, it waits once more and differences against the
/// same baseline, which gives a longer window. If that also gives nothing, it returns
/// an error rather than the since-boot figure.
#[cfg(target_os = "macos")]
pub fn current_total_usage() -> crate::error::Result<(CoreUsage, Vec<Option<CoreUsage>>)> {
    use crate::error::IronError;

    let read = || -> crate::error::Result<Vec<CoreTicks>> {
        per_core_ticks().ok_or_else(|| {
            IronError::UnsupportedPlatform("host_processor_info returned no per-core ticks".into())
        })
    };

    // Held across the waits so two callers cannot prime the cache at once.
    let mut previous = PREVIOUS_CORES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    let baseline: Vec<CoreTicks> = match previous.as_ref() {
        Some(prior) => prior.clone(),
        None => {
            let first = read()?;
            std::thread::sleep(PRIME_INTERVAL);
            first
        }
    };

    for attempt in 0..2 {
        if attempt > 0 {
            std::thread::sleep(PRIME_INTERVAL);
        }
        let now = read()?;
        let total = usage_between(&aggregate_ticks(&baseline), &aggregate_ticks(&now));
        let cores = per_core_usage_between(&baseline, &now);
        *previous = Some(now);
        if let Some(usage) = total {
            return Ok((usage, cores));
        }
    }

    Err(IronError::Parse(
        concat!(
            "per-core tick counters gave no usable split: they did not advance, ",
            "or went backwards, between reads"
        )
        .into(),
    ))
}

/// Per-core cumulative ticks from `host_processor_info`.
///
/// This is the one piece of FFI in the module. It earns its place where `top`
/// parsing could not: it is the only source of per-core figures and of nice time,
/// and unlike a hand-rolled Mach structure it is a call whose signature the
/// compiler checks against `libc` and whose result `tests/macos_readers.rs`
/// checks against reality on `macos-latest`.
///
/// Returns `None` rather than partial data if the call fails.
#[cfg(target_os = "macos")]
pub fn per_core_ticks() -> Option<Vec<CoreTicks>> {
    use std::ptr;

    let mut cpu_count: libc::natural_t = 0;
    let mut info: libc::processor_info_array_t = ptr::null_mut();
    let mut info_count: libc::mach_msg_type_number_t = 0;

    // SAFETY: out-parameters are all initialised above and passed by address. On
    // success the kernel allocates `info`, which is released via vm_deallocate
    // below on every path that reaches it.
    let result = unsafe {
        libc::host_processor_info(
            libc::mach_host_self(),
            libc::PROCESSOR_CPU_LOAD_INFO,
            &mut cpu_count,
            &mut info,
            &mut info_count,
        )
    };

    if result != libc::KERN_SUCCESS || info.is_null() {
        return None;
    }

    let states = libc::CPU_STATE_MAX as usize;
    // SAFETY: the kernel reports how many integers it wrote in `info_count`.
    let data =
        unsafe { std::slice::from_raw_parts(info as *const libc::integer_t, info_count as usize) };

    let mut cores = Vec::with_capacity(cpu_count as usize);
    for i in 0..cpu_count as usize {
        let base = i * states;
        if base + states > data.len() {
            // The kernel reported more processors than it wrote data for. Stop at
            // what was actually returned rather than reading past the buffer.
            break;
        }
        cores.push(CoreTicks {
            user: data[base + libc::CPU_STATE_USER as usize].max(0) as u64,
            system: data[base + libc::CPU_STATE_SYSTEM as usize].max(0) as u64,
            idle: data[base + libc::CPU_STATE_IDLE as usize].max(0) as u64,
            nice: data[base + libc::CPU_STATE_NICE as usize].max(0) as u64,
        });
    }

    // SAFETY: `info` was allocated by host_processor_info and is released exactly
    // once here. Leaking it would grow the task's VM on every sample, which for a
    // monitor polling once a second is a leak that compounds all day.
    unsafe {
        libc::vm_deallocate(
            libc::mach_task_self(),
            info as libc::vm_address_t,
            info_count as usize * std::mem::size_of::<libc::integer_t>(),
        );
    }

    Some(cores)
}

/// Run a command and return stdout, or `None` if it could not be run.
#[cfg(target_os = "macos")]
fn run(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

#[cfg(not(target_os = "macos"))]
#[allow(dead_code)]
fn run(_program: &str, _args: &[&str]) -> Option<String> {
    // Present so the module compiles everywhere and its parsers stay testable.
    let _ = Command::new("true");
    None
}

/// Read a single numeric `sysctl` value.
pub fn sysctl_u64(name: &str) -> Option<u64> {
    run("sysctl", &["-n", name])?.trim().parse().ok()
}

/// Read a single string `sysctl` value.
pub fn sysctl_string(name: &str) -> Option<String> {
    run("sysctl", &["-n", name]).map(|s| s.trim().to_string())
}

/// Aggregate CPU usage over a one-second window, or `None` if `top` could not be read
/// or understood.
///
/// `-l 1` would be a single sample, and `top`'s first sample is the since-boot
/// average, so this takes two and [`parse_top_cpu`] keeps the second.
pub fn cpu_usage() -> Option<CpuUsage> {
    parse_top_cpu(&run("top", &["-l", "2", "-s", "1", "-n", "0"])?)
}

/// Virtual memory page counts, or `None` if `vm_stat` could not be read.
pub fn vm_stat() -> Option<VmStat> {
    parse_vm_stat(&run("vm_stat", &[])?)
}

/// Swap usage, or `None` if `vm.swapusage` could not be read.
pub fn swap_usage() -> Option<SwapUsage> {
    parse_swapusage(&run("sysctl", &["-n", "vm.swapusage"])?)
}

/// Assemble [`CpuStats`] the way the ontology resolver expects it.
///
/// This existed as scattered primitives — `per_core_ticks`, `aggregate_ticks`,
/// `sysctl_string` — and nothing assembled them, so
/// `ontology::resolve::read_cpu_stats` had no macOS arm and returned `None`. The
/// resolver then reported `cpu.total.utilization` as "the platform CPU reader
/// returned an error", which was not true: no reader had been called. Four more
/// entities came back as "no resolver bound on this build" while the readers sat
/// here unused.
///
/// Per-core ticks are the source rather than `top`, because they are the only
/// one carrying nice time. The whole-CPU split is the difference between two of
/// their readings, see [`current_total_usage`].
#[cfg(target_os = "macos")]
pub fn read_cpu_stats() -> crate::error::Result<crate::core::cpu::CpuStats> {
    use crate::core::cpu::{CpuCore, CpuStats, CpuTotal};

    let (total_usage, per_core) = current_total_usage()?;

    // Only the CPU brand string. An earlier version fell back to `hw.model`,
    // which is the *machine* identifier — "Mac14,2" — and reporting that as the
    // CPU model is a wrong reading rather than an absence. It also made the
    // result unreadable: CI went green and there was no way to tell which of the
    // two sysctls had answered, so `cpu.model` might have been a processor name
    // or a chassis code and nothing distinguished them.
    //
    // `None` when the key is missing, which the resolver turns into an
    // unavailable reading carrying a reason. That is the honest outcome and the
    // one this crate's ontology asks for. It used to be `""`.
    let model = sysctl_string("machdep.cpu.brand_string").filter(|s| !s.is_empty());

    // Each core's split is the difference between its own two readings over the same
    // window as the total. A core with no split (it did not move, or the core count
    // changed) is `None`, never its since-boot share, which is an average over uptime.
    // The `top` fallback has no per-core readings and leaves every core `None`.
    let cores = per_core
        .iter()
        .enumerate()
        .map(|(id, usage)| CpuCore {
            id,
            // No macOS reader fills this. It was a literal `true`.
            online: None,
            // macOS exposes no governor. `None`, not a placeholder.
            governor: None,
            // No unelevated source of live per-core frequency on macOS.
            // `None` says that; a nominal figure would repeat the mistake
            // Windows makes with `CurrentMhz`.
            frequency: None,
            user: usage.map(|u| u.user),
            nice: usage.map(|u| u.nice),
            system: usage.map(|u| u.system),
            idle: usage.map(|u| u.idle),
            model: model.clone(),
        })
        .collect();

    Ok(CpuStats {
        cores,
        total: CpuTotal {
            user: total_usage.user,
            // The kernel counts nice time, and the split covers the same window
            // as the other fields.
            nice: Some(total_usage.nice),
            system: total_usage.system,
            idle: total_usage.idle,
        },
    })
}

/// Assemble [`MemoryStats`] from `vm_stat`, `hw.memsize` and `vm.swapusage`.
///
/// Units: the core structs are in KB, `vm_stat` counts pages and `swapusage`
/// reports bytes, so both are converted here rather than at the call site.
#[cfg(target_os = "macos")]
pub fn read_memory_stats() -> crate::error::Result<crate::core::memory::MemoryStats> {
    use crate::core::memory::{MemoryStats, RamInfo, SwapInfo};

    let vm = vm_stat().ok_or_else(|| {
        crate::error::IronError::Parse("vm_stat produced no parseable output".into())
    })?;
    let total_bytes = sysctl_u64("hw.memsize").ok_or_else(|| {
        crate::error::IronError::UnsupportedPlatform("hw.memsize was not readable".into())
    })?;

    // A machine with swap disabled reports zeros through `vm.swapusage`, and
    // that is a reading. A failed read is not, and `SwapInfo` can now say so:
    // `None` rather than zero.
    //
    // Until 6.0.0 this failed the entire memory read when the sysctl was
    // unreadable, losing the RAM figures to avoid claiming the machine had no
    // swap. That trade existed only because the type could not express
    // "unknown", and it is gone now that it can.
    let swap = swap_usage();

    Ok(MemoryStats {
        ram: RamInfo {
            total: total_bytes / 1024,
            used: vm.used_bytes() / 1024,
            free: Some(vm.free_bytes() / 1024),
            // macOS keeps no buffer pool distinct from the file cache, so zero
            // here is a fact about the platform rather than a missing reading.
            // No macOS equivalent of the Linux "Buffers" line.
            buffers: None,
            cached: Some(vm.cached_bytes() / 1024),
            // macOS has shared memory and IronMonitor does not read it, so `None`.
            // This was a zero until 6.0.0, which read as a measurement of none.
            shared: None,
            lfb: None,
        },
        swap: SwapInfo {
            total: swap.map(|s| s.total / 1024),
            used: swap.map(|s| s.used / 1024),
            // macOS reports no cached-swap figure.
            cached: None,
        },
        emc: None,
        iram: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Captured from `top -l 1 -n 0`.
    const TOP_OUTPUT: &str = "Processes: 512 total, 2 running, 510 sleeping, 2841 threads
2026/08/06 21:14:02
Load Avg: 2.31, 2.55, 2.72
CPU usage: 4.76% user, 9.52% sys, 85.71% idle
SharedLibs: 512M resident, 88M data, 42M linkedit.
MemRegions: 123456 total, 4321M resident, 234M private, 1234M shared.
";

    #[test]
    fn top_cpu_line_is_split_into_user_system_and_idle() {
        let usage = parse_top_cpu(TOP_OUTPUT).expect("parses");
        assert!((usage.user - 4.76).abs() < 0.001);
        assert!((usage.system - 9.52).abs() < 0.001);
        assert!((usage.idle - 85.71).abs() < 0.001);
    }

    /// If `top`'s wording changes, the right answer is "unknown", not "idle".
    #[test]
    fn a_missing_cpu_line_is_none_rather_than_zero_usage() {
        assert!(parse_top_cpu("Processes: 512 total\nLoad Avg: 1.0\n").is_none());
        assert!(parse_top_cpu("").is_none());
    }

    /// A split that does not sum to roughly 100% means the line was misread.
    #[test]
    fn an_implausible_split_is_rejected() {
        let bad = "CPU usage: 4.00% user, 5.00% sys, 6.00% idle";
        assert!(parse_top_cpu(bad).is_none());
    }

    /// Captured from `vm_stat` on Apple Silicon, where the page size is 16384 —
    /// assuming 4096 would understate every figure by a factor of four.
    const VM_STAT_OUTPUT: &str = "Mach Virtual Memory Statistics: (page size of 16384 bytes)
Pages free:                               45678.
Pages active:                            234567.
Pages inactive:                          123456.
Pages speculative:                        12345.
Pages throttled:                              0.
Pages wired down:                         98765.
Pages purgeable:                           4321.
\"Translation faults\":                 987654321.
Pages copy-on-write:                   12345678.
Pages zero filled:                    234567890.
Pages reactivated:                      1234567.
Pages purged:                            123456.
File-backed pages:                        87654.
Anonymous pages:                         270369.
Pages stored in compressor:              456789.
Pages occupied by compressor:             34567.
";

    #[test]
    fn vm_stat_reads_the_page_size_from_the_header() {
        let vm = parse_vm_stat(VM_STAT_OUTPUT).expect("parses");
        assert_eq!(vm.page_size, 16384, "Apple Silicon uses 16K pages");
    }

    #[test]
    fn vm_stat_reads_each_page_count() {
        let vm = parse_vm_stat(VM_STAT_OUTPUT).expect("parses");
        assert_eq!(vm.free, 45678);
        assert_eq!(vm.active, 234567);
        assert_eq!(vm.inactive, 123456);
        assert_eq!(vm.speculative, 12345);
        assert_eq!(vm.wired, 98765);
        assert_eq!(vm.compressed, 34567);
        assert_eq!(vm.file_backed, 87654);
    }

    /// "Pages stored in compressor" and "Pages occupied by compressor" are
    /// different quantities and adjacent lines; the second is the resident cost.
    #[test]
    fn compressed_pages_are_the_occupied_count_not_the_stored_count() {
        let vm = parse_vm_stat(VM_STAT_OUTPUT).expect("parses");
        assert_eq!(vm.compressed, 34567, "occupied, not the 456789 stored");
    }

    #[test]
    fn used_memory_counts_active_wired_and_compressed() {
        let vm = parse_vm_stat(VM_STAT_OUTPUT).expect("parses");
        assert_eq!(vm.used_bytes(), (234567 + 98765 + 34567) * 16384);
        assert_eq!(vm.free_bytes(), (45678 + 12345) * 16384);
        assert_eq!(vm.cached_bytes(), 87654 * 16384);
    }

    /// A missing line used to parse as `0`: without "File-backed pages",
    /// cached memory read as none at all, with nothing to say it was not read.
    #[test]
    fn a_vm_stat_missing_a_line_is_rejected_rather_than_read_as_zero() {
        let without: String = VM_STAT_OUTPUT
            .lines()
            .filter(|l| !l.starts_with("File-backed pages"))
            .map(|l| format!("{l}\n"))
            .collect();
        assert!(parse_vm_stat(&without).is_none());
    }

    #[test]
    fn a_vm_stat_without_a_page_size_header_is_rejected() {
        assert!(parse_vm_stat("Pages free: 100.\n").is_none());
    }

    #[test]
    fn swapusage_scales_by_the_suffix_it_reports() {
        let swap =
            parse_swapusage("total = 2048.00M  used = 1024.50M  free = 1023.50M  (encrypted)")
                .expect("parses");
        assert_eq!(swap.total, 2048 * 1024 * 1024);
        assert_eq!(swap.used, (1024.5 * 1024.0 * 1024.0) as u64);
    }

    #[test]
    fn swapusage_handles_gigabyte_suffixes() {
        let swap =
            parse_swapusage("total = 8.00G  used = 2.50G  free = 5.50G  (encrypted)").expect("ok");
        assert_eq!(swap.total, 8 * 1024 * 1024 * 1024);
        assert_eq!(swap.used, (2.5 * 1024.0 * 1024.0 * 1024.0) as u64);
    }

    /// Swap disabled is a reading of zero, not an absence.
    #[test]
    fn swap_disabled_reports_zero_rather_than_none() {
        let swap = parse_swapusage("total = 0.00M  used = 0.00M  free = 0.00M").expect("parses");
        assert_eq!(swap.total, 0);
        assert_eq!(swap.used, 0);
    }

    #[test]
    fn unparseable_swapusage_is_none() {
        assert!(parse_swapusage("vm.swapusage: unavailable").is_none());
    }

    #[test]
    fn ticks_become_percentages_of_that_core() {
        let ticks = CoreTicks {
            user: 250,
            system: 150,
            idle: 550,
            nice: 50,
        };
        let usage = ticks.percentages().expect("has accumulated time");
        assert!((usage.user - 25.0).abs() < 0.01);
        assert!((usage.system - 15.0).abs() < 0.01);
        assert!((usage.idle - 55.0).abs() < 0.01);
        assert!((usage.nice - 5.0).abs() < 0.01);
        assert!((usage.user + usage.nice + usage.system + usage.idle - 100.0).abs() < 0.01);
    }

    /// A core with no accumulated time was not read. Reporting 0% used and 0% idle
    /// would present that as a measurement of an idle core.
    #[test]
    fn a_core_with_no_ticks_is_none_rather_than_wholly_idle() {
        assert_eq!(CoreTicks::default().percentages(), None);
    }

    #[test]
    fn aggregate_sums_every_core() {
        let cores = [
            CoreTicks {
                user: 10,
                system: 20,
                idle: 30,
                nice: 40,
            },
            CoreTicks {
                user: 1,
                system: 2,
                idle: 3,
                nice: 4,
            },
        ];
        let total = aggregate_ticks(&cores);
        assert_eq!(total.user, 11);
        assert_eq!(total.system, 22);
        assert_eq!(total.idle, 33);
        assert_eq!(total.nice, 44);
    }

    /// Counters near u64::MAX must not wrap a busy machine into an idle one.
    #[test]
    fn aggregating_saturates_rather_than_wrapping() {
        let cores = [
            CoreTicks {
                user: u64::MAX,
                ..Default::default()
            },
            CoreTicks {
                user: 100,
                ..Default::default()
            },
        ];
        assert_eq!(aggregate_ticks(&cores).user, u64::MAX);
    }

    #[test]
    fn an_empty_core_list_aggregates_to_no_reading() {
        assert_eq!(aggregate_ticks(&[]).percentages(), None);
    }

    /// Aggregate counters for an 8-core machine that has been up for days.
    fn since_boot() -> CoreTicks {
        CoreTicks {
            user: 1_804_512,
            system: 912_377,
            idle: 21_006_418,
            nice: 3_921,
        }
    }

    /// The split is the growth over the window, not the counters' total.
    #[test]
    fn usage_between_is_the_split_of_the_window_only() {
        let earlier = since_boot();
        let later = CoreTicks {
            user: earlier.user + 250,
            system: earlier.system + 150,
            idle: earlier.idle + 550,
            nice: earlier.nice + 50,
        };
        let usage = usage_between(&earlier, &later).expect("counters advanced");
        assert!((usage.user - 25.0).abs() < 0.01);
        assert!((usage.system - 15.0).abs() < 0.01);
        assert!((usage.idle - 55.0).abs() < 0.01);
        assert!((usage.nice - 5.0).abs() < 0.01);

        // The since-boot figure of the same later reading is a different answer,
        // which is why reporting it as current load was wrong.
        let since_boot_idle = later.percentages().expect("has time").idle;
        assert!(since_boot_idle > 70.0);
        assert!((usage.idle - since_boot_idle).abs() > 10.0);
    }

    /// Each core is split over its own window, and a core that did not move, or went
    /// backwards, is `None` without taking the others down with it.
    #[test]
    fn per_core_splits_pair_each_core_with_its_own_earlier_reading() {
        let core_a = since_boot();
        let core_b = CoreTicks {
            user: 9_000,
            system: 1_000,
            idle: 90_000,
            nice: 0,
        };
        let later_a = CoreTicks {
            user: core_a.user + 250,
            system: core_a.system + 150,
            idle: core_a.idle + 550,
            nice: core_a.nice + 50,
        };
        // Core B did not move, so it has no split of its own.
        let splits = per_core_usage_between(&[core_a, core_b], &[later_a, core_b]);
        assert_eq!(splits.len(), 2);
        let a = splits[0].expect("core A advanced");
        assert!((a.user - 25.0).abs() < 0.01, "user {}", a.user);
        assert!((a.idle - 55.0).abs() < 0.01, "idle {}", a.idle);
        assert_eq!(splits[1], None);
    }

    /// A change in the core count cannot be paired entry by entry, so no core has a split.
    #[test]
    fn a_core_count_change_gives_no_per_core_split() {
        let core = since_boot();
        let later = CoreTicks {
            user: core.user + 250,
            system: core.system + 150,
            idle: core.idle + 550,
            nice: core.nice + 50,
        };
        assert_eq!(
            per_core_usage_between(&[core], &[later, later]),
            vec![None, None]
        );
        assert_eq!(per_core_usage_between(&[core, core], &[later]), vec![None]);
    }

    /// Two reads inside one tick: nothing moved, so there is no split to report.
    #[test]
    fn a_zero_delta_is_no_split_rather_than_the_since_boot_figure() {
        let earlier = since_boot();
        assert_eq!(usage_between(&earlier, &earlier), None);
    }

    /// A counter that decreased means the readings are not comparable.
    #[test]
    fn a_counter_that_went_backwards_is_no_split() {
        let earlier = since_boot();
        let later = CoreTicks {
            user: earlier.user - 1,
            system: earlier.system + 400,
            idle: earlier.idle + 600,
            nice: earlier.nice,
        };
        assert_eq!(usage_between(&earlier, &later), None);
    }

    /// `top` prints one `CPU usage:` line per sample. The first is since boot.
    #[test]
    fn top_takes_the_last_sample_not_the_since_boot_one() {
        const TWO_SAMPLES: &str = "Processes: 498 total, 2 running, 496 sleeping, 2790 threads
2026/08/06 21:14:02
Load Avg: 2.31, 2.55, 2.72
CPU usage: 4.76% user, 9.52% sys, 85.71% idle
SharedLibs: 512M resident, 88M data, 42M linkedit.
MemRegions: 123456 total, 4321M resident, 234M private, 1234M shared.

Processes: 499 total, 3 running, 496 sleeping, 2793 threads
2026/08/06 21:14:03
Load Avg: 2.36, 2.56, 2.72
CPU usage: 31.25% user, 12.50% sys, 56.25% idle
SharedLibs: 512M resident, 88M data, 42M linkedit.
MemRegions: 123456 total, 4322M resident, 234M private, 1234M shared.
";
        let usage = parse_top_cpu(TWO_SAMPLES).expect("parses");
        assert!((usage.user - 31.25).abs() < 0.01);
        assert!((usage.system - 12.5).abs() < 0.01);
        assert!((usage.idle - 56.25).abs() < 0.01);
    }
}
