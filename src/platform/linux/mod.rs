//! Linux platform implementation

pub mod cpu;
pub mod gpu;
pub mod jetson;
pub mod memory;
pub mod platform_detect;
pub mod power;
pub mod temperature;

pub use cpu::read_cpu_stats;
pub use gpu::read_gpu_stats;
pub use memory::read_memory_stats;
pub use platform_detect::detect_platform;
pub use power::read_power_stats;
pub use temperature::read_temperature_stats;

/// Bytes per memory page, from `sysconf(_SC_PAGESIZE)`.
///
/// `/proc/<pid>/statm` counts pages, and the readers multiplied by a literal
/// 4096. That is x86's page size and a common ARM64 one, not every machine's:
/// ARM64 kernels are also built with 16 KiB pages (Apple Silicon under Asahi)
/// and 64 KiB pages (several server distributions), where every process's
/// memory read four or sixteen times too small.
pub fn page_size() -> Option<u64> {
    // SAFETY: `sysconf` takes a constant and has no preconditions.
    let size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    u64::try_from(size).ok().filter(|&s| s > 0)
}

/// Clock ticks per second, from `sysconf(_SC_CLK_TCK)`: the unit of the CPU
/// times in `/proc/<pid>/stat`.
///
/// The readers divided by a literal 100. That is `USER_HZ` on most kernels,
/// and it is a kernel configuration value rather than a constant.
pub fn clock_ticks_per_second() -> Option<u64> {
    // SAFETY: as for `page_size`.
    let ticks = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    u64::try_from(ticks).ok().filter(|&t| t > 0)
}

#[cfg(test)]
mod sysconf_tests {
    /// Both are answered on every Linux system; `None` would mean the call
    /// itself is wrong.
    #[test]
    fn page_size_and_tick_rate_are_read_from_the_system() {
        let page = super::page_size().expect("page size");
        assert!(page.is_power_of_two() && page >= 4096, "page size {page}");
        let ticks = super::clock_ticks_per_second().expect("clock ticks");
        assert!(ticks > 0, "ticks {ticks}");
    }
}
