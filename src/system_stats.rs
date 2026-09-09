//! System-wide statistics (Linux/BSD style)
//!
//! This module provides system-wide statistics similar to popular Unix monitoring tools:
//! - Load average (1, 5, 15 min) - like `uptime`, `htop`, `top`
//! - Context switches and interrupts - like `vmstat`
//! - System uptime - like `uptime`
//! - Boot time
//! - Running/total processes - like `top`
//!
//! # Examples
//!
//! ```no_run
//! use ironmonlib::SystemStats;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let stats = SystemStats::new()?;
//!
//! // Load average (like uptime command)
//! if let Some(load) = &stats.load_average {
//!     println!("Load: {:.2} {:.2} {:.2}",
//!         load.one, load.five, load.fifteen);
//! }
//!
//! // Uptime
//! if let Some(uptime) = stats.uptime_seconds {
//!     let days = uptime / 86400;
//!     let hours = (uptime % 86400) / 3600;
//!     println!("Uptime: {} days, {} hours", days, hours);
//! }
//! # Ok(())
//! # }
//! ```

use crate::error::Result;
use serde::{Deserialize, Serialize};

/// Load average values (1, 5, 15 minute)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadAverage {
    /// 1 minute load average
    pub one: f64,
    /// 5 minute load average
    pub five: f64,
    /// 15 minute load average
    pub fifteen: f64,
}

/// CPU time breakdown (like /proc/stat)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuTime {
    /// Time spent in user mode
    pub user: u64,
    /// Time spent in user mode with low priority (nice)
    pub nice: u64,
    /// Time spent in system mode
    pub system: u64,
    /// Time spent idle
    pub idle: u64,
    /// Time waiting for I/O (Linux)
    pub iowait: u64,
    /// Time servicing hardware interrupts (Linux)
    pub irq: u64,
    /// Time servicing software interrupts (Linux)
    pub softirq: u64,
    /// Time stolen by hypervisor (Linux)
    pub steal: u64,
    /// Time spent in guest mode (Linux)
    pub guest: u64,
    /// Time spent in guest nice mode (Linux)
    pub guest_nice: u64,
}

impl CpuTime {
    /// Total CPU time
    pub fn total(&self) -> u64 {
        self.user
            + self.nice
            + self.system
            + self.idle
            + self.iowait
            + self.irq
            + self.softirq
            + self.steal
    }

    /// Total busy time (non-idle)
    pub fn busy(&self) -> u64 {
        self.user + self.nice + self.system + self.irq + self.softirq + self.steal
    }
}

/// Virtual memory statistics (like vmstat)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmStats {
    /// Pages paged in
    pub pages_in: u64,
    /// Pages paged out
    pub pages_out: u64,
    /// Pages swapped in
    pub swap_in: u64,
    /// Pages swapped out
    pub swap_out: u64,
    /// Interrupts per second
    pub interrupts: u64,
    /// Context switches per second
    pub context_switches: u64,
    /// Processes created (forks)
    pub processes_created: u64,
    /// Processes currently running
    pub processes_running: u32,
    /// Processes currently blocked
    pub processes_blocked: u32,
}

/// System-wide statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStats {
    /// Load average (1, 5, 15 minute)
    pub load_average: Option<LoadAverage>,
    /// System uptime in seconds
    pub uptime_seconds: Option<u64>,
    /// Idle time in seconds (sum across all CPUs)
    pub idle_seconds: Option<u64>,
    /// Boot time (Unix timestamp)
    pub boot_time: Option<u64>,
    /// Number of CPUs/cores
    pub num_cpus: u32,
    /// Total number of processes, or `None` where it was not read.
    ///
    /// This was a bare `u32` and only Linux ever assigned it, so Windows and
    /// macOS reported `0` -- no processes at all, on a machine running several
    /// hundred. Windows now answers it from `GetPerformanceInfo`; macOS still
    /// does not, and says so.
    pub total_processes: Option<u32>,
    /// Number of processes in the running state, or `None` where unreported.
    ///
    /// Linux takes this from `procs_running` in `/proc/stat`. Neither Windows
    /// nor macOS publishes a runnable count, so it stays `None` there rather
    /// than claiming nothing is running.
    pub running_processes: Option<u32>,
    /// CPU time breakdown
    pub cpu_time: Option<CpuTime>,
    /// Virtual memory stats
    pub vm_stats: Option<VmStats>,
    /// Hostname
    pub hostname: Option<String>,
    /// Kernel version
    pub kernel_version: Option<String>,
}

impl SystemStats {
    /// Create a new SystemStats instance with current values
    pub fn new() -> Result<Self> {
        #[cfg(target_os = "linux")]
        {
            linux::read_system_stats()
        }

        #[cfg(target_os = "windows")]
        {
            windows::read_system_stats()
        }

        #[cfg(target_os = "macos")]
        {
            macos::read_system_stats()
        }

        #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
        {
            Err(IronError::NotImplemented(
                "System stats not implemented for this platform".into(),
            ))
        }
    }

    /// Get load average as formatted string
    pub fn load_string(&self) -> String {
        if let Some(ref load) = self.load_average {
            format!("{:.2}, {:.2}, {:.2}", load.one, load.five, load.fifteen)
        } else {
            "N/A".to_string()
        }
    }

    /// Get uptime as formatted string
    pub fn uptime_string(&self) -> String {
        if let Some(secs) = self.uptime_seconds {
            let days = secs / 86400;
            let hours = (secs % 86400) / 3600;
            let minutes = (secs % 3600) / 60;
            if days > 0 {
                format!("{} days, {:02}:{:02}", days, hours, minutes)
            } else {
                format!("{:02}:{:02}:{:02}", hours, minutes, secs % 60)
            }
        } else {
            "N/A".to_string()
        }
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::fs;

    pub fn read_system_stats() -> Result<SystemStats> {
        let mut stats = SystemStats {
            load_average: None,
            uptime_seconds: None,
            idle_seconds: None,
            boot_time: None,
            num_cpus: 0,
            total_processes: None,
            running_processes: None,
            cpu_time: None,
            vm_stats: None,
            hostname: None,
            kernel_version: None,
        };

        // Read load average from /proc/loadavg
        if let Ok(content) = fs::read_to_string("/proc/loadavg") {
            let parts: Vec<&str> = content.split_whitespace().collect();
            if parts.len() >= 5 {
                stats.load_average = Some(LoadAverage {
                    one: parts[0].parse().unwrap_or(0.0),
                    five: parts[1].parse().unwrap_or(0.0),
                    fifteen: parts[2].parse().unwrap_or(0.0),
                });

                // Parse running/total processes from "1/234" format
                if let Some((running, total)) = parts[3].split_once('/') {
                    stats.running_processes = running.parse().ok();
                    stats.total_processes = total.parse().ok();
                }
            }
        }

        // Read uptime from /proc/uptime
        if let Ok(content) = fs::read_to_string("/proc/uptime") {
            let parts: Vec<&str> = content.split_whitespace().collect();
            if parts.len() >= 2 {
                stats.uptime_seconds = parts[0].parse::<f64>().ok().map(|v| v as u64);
                stats.idle_seconds = parts[1].parse::<f64>().ok().map(|v| v as u64);
            }
        }

        // Read CPU stats from /proc/stat
        if let Ok(content) = fs::read_to_string("/proc/stat") {
            for line in content.lines() {
                if line.starts_with("cpu ") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 8 {
                        stats.cpu_time = Some(CpuTime {
                            user: parts[1].parse().unwrap_or(0),
                            nice: parts[2].parse().unwrap_or(0),
                            system: parts[3].parse().unwrap_or(0),
                            idle: parts[4].parse().unwrap_or(0),
                            iowait: parts.get(5).and_then(|s| s.parse().ok()).unwrap_or(0),
                            irq: parts.get(6).and_then(|s| s.parse().ok()).unwrap_or(0),
                            softirq: parts.get(7).and_then(|s| s.parse().ok()).unwrap_or(0),
                            steal: parts.get(8).and_then(|s| s.parse().ok()).unwrap_or(0),
                            guest: parts.get(9).and_then(|s| s.parse().ok()).unwrap_or(0),
                            guest_nice: parts.get(10).and_then(|s| s.parse().ok()).unwrap_or(0),
                        });
                    }
                } else if line.starts_with("cpu") && !line.starts_with("cpu ") {
                    stats.num_cpus += 1;
                } else if line.starts_with("btime ") {
                    stats.boot_time = line.split_whitespace().nth(1).and_then(|s| s.parse().ok());
                } else if line.starts_with("ctxt ") {
                    if stats.vm_stats.is_none() {
                        stats.vm_stats = Some(VmStats {
                            pages_in: 0,
                            pages_out: 0,
                            swap_in: 0,
                            swap_out: 0,
                            interrupts: 0,
                            context_switches: 0,
                            processes_created: 0,
                            processes_running: 0,
                            processes_blocked: 0,
                        });
                    }
                    if let Some(ref mut vm) = stats.vm_stats {
                        vm.context_switches = line
                            .split_whitespace()
                            .nth(1)
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(0);
                    }
                } else if line.starts_with("intr ") {
                    if let Some(ref mut vm) = stats.vm_stats {
                        vm.interrupts = line
                            .split_whitespace()
                            .nth(1)
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(0);
                    }
                } else if line.starts_with("processes ") {
                    if let Some(ref mut vm) = stats.vm_stats {
                        vm.processes_created = line
                            .split_whitespace()
                            .nth(1)
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(0);
                    }
                } else if line.starts_with("procs_running ") {
                    if let Some(ref mut vm) = stats.vm_stats {
                        vm.processes_running = line
                            .split_whitespace()
                            .nth(1)
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(0);
                    }
                } else if line.starts_with("procs_blocked ") {
                    if let Some(ref mut vm) = stats.vm_stats {
                        vm.processes_blocked = line
                            .split_whitespace()
                            .nth(1)
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(0);
                    }
                }
            }
        }

        // Read vmstat for page in/out
        if let Ok(content) = fs::read_to_string("/proc/vmstat") {
            if stats.vm_stats.is_none() {
                stats.vm_stats = Some(VmStats {
                    pages_in: 0,
                    pages_out: 0,
                    swap_in: 0,
                    swap_out: 0,
                    interrupts: 0,
                    context_switches: 0,
                    processes_created: 0,
                    processes_running: 0,
                    processes_blocked: 0,
                });
            }
            for line in content.lines() {
                if let Some(ref mut vm) = stats.vm_stats {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        match parts[0] {
                            "pgpgin" => vm.pages_in = parts[1].parse().unwrap_or(0),
                            "pgpgout" => vm.pages_out = parts[1].parse().unwrap_or(0),
                            "pswpin" => vm.swap_in = parts[1].parse().unwrap_or(0),
                            "pswpout" => vm.swap_out = parts[1].parse().unwrap_or(0),
                            _ => {}
                        }
                    }
                }
            }
        }

        // Read hostname
        if let Ok(content) = fs::read_to_string("/proc/sys/kernel/hostname") {
            stats.hostname = Some(content.trim().to_string());
        }

        // Read kernel version
        if let Ok(content) = fs::read_to_string("/proc/version") {
            // Extract just the version number
            if let Some(version) = content.split_whitespace().nth(2) {
                stats.kernel_version = Some(version.to_string());
            }
        }

        Ok(stats)
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use super::*;

    pub fn read_system_stats() -> Result<SystemStats> {
        use ::windows::Win32::System::SystemInformation::{
            GetSystemInfo, GetTickCount64, SYSTEM_INFO,
        };
        use std::mem::MaybeUninit;

        let mut stats = SystemStats {
            load_average: None,
            uptime_seconds: None,
            idle_seconds: None,
            boot_time: None,
            num_cpus: 0,
            total_processes: None,
            running_processes: None,
            cpu_time: None,
            vm_stats: None,
            hostname: None,
            kernel_version: None,
        };

        // Get system info for CPU count
        unsafe {
            let mut sys_info: MaybeUninit<SYSTEM_INFO> = MaybeUninit::uninit();
            GetSystemInfo(sys_info.as_mut_ptr());
            let sys_info = sys_info.assume_init();
            stats.num_cpus = sys_info.dwNumberOfProcessors;
        }

        // Process count, from the struct this crate was already calling for the
        // system file cache. `PERFORMANCE_INFORMATION.ProcessCount` is exactly
        // this figure and costs nothing extra to ask for.
        {
            use ::windows::Win32::System::ProcessStatus::{
                GetPerformanceInfo, PERFORMANCE_INFORMATION,
            };
            let mut perf: PERFORMANCE_INFORMATION = unsafe { std::mem::zeroed() };
            perf.cb = std::mem::size_of::<PERFORMANCE_INFORMATION>() as u32;
            if unsafe { GetPerformanceInfo(&mut perf, perf.cb) }.is_ok() {
                stats.total_processes = Some(perf.ProcessCount);
            }
        }
        // Windows publishes no runnable-process count, so `running_processes`
        // stays `None`.

        // Get uptime via GetTickCount64
        unsafe {
            let tick_count = GetTickCount64();
            stats.uptime_seconds = Some(tick_count / 1000);
        }

        // Get hostname
        #[cfg(feature = "cli")]
        {
            stats.hostname = hostname::get()
                .ok()
                .map(|h| h.to_string_lossy().to_string());
        }
        #[cfg(feature = "gui")]
        {
            if stats.hostname.is_none() {
                stats.hostname = hostname::get()
                    .ok()
                    .map(|h| h.to_string_lossy().to_string());
            }
        }

        // Windows doesn't have traditional load average, but we can simulate with processor queue length
        // For now, leave it as None - would require PDH counters

        Ok(stats)
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;

    pub fn read_system_stats() -> Result<SystemStats> {
        let mut stats = SystemStats {
            load_average: None,
            uptime_seconds: None,
            idle_seconds: None,
            boot_time: None,
            num_cpus: 0,
            total_processes: None,
            running_processes: None,
            cpu_time: None,
            vm_stats: None,
            hostname: None,
            kernel_version: None,
        };

        // macOS has getloadavg
        let mut loadavg: [f64; 3] = [0.0; 3];
        unsafe {
            let result = libc::getloadavg(loadavg.as_mut_ptr(), 3);
            if result == 3 {
                stats.load_average = Some(LoadAverage {
                    one: loadavg[0],
                    five: loadavg[1],
                    fifteen: loadavg[2],
                });
            }
        }

        // Get number of CPUs
        #[cfg(feature = "num_cpus")]
        {
            stats.num_cpus = num_cpus::get() as u32;
        }
        #[cfg(not(feature = "num_cpus"))]
        {
            stats.num_cpus = 1; // Fallback
        }

        // macOS reports no process count here. `sysctl kern.proc` would give
        // one, and until somebody writes it this stays absent rather than 0.

        // Get hostname
        #[cfg(feature = "hostname")]
        {
            stats.hostname = hostname::get()
                .ok()
                .map(|h| h.to_string_lossy().to_string());
        }

        Ok(stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cpu_time_total() {
        let ct = CpuTime {
            user: 100,
            nice: 10,
            system: 50,
            idle: 200,
            iowait: 5,
            irq: 3,
            softirq: 2,
            steal: 1,
            guest: 0,
            guest_nice: 0,
        };
        assert_eq!(ct.total(), 100 + 10 + 50 + 200 + 5 + 3 + 2 + 1);
    }

    #[test]
    fn test_cpu_time_busy() {
        let ct = CpuTime {
            user: 100,
            nice: 10,
            system: 50,
            idle: 200,
            iowait: 5,
            irq: 3,
            softirq: 2,
            steal: 1,
            guest: 0,
            guest_nice: 0,
        };
        assert_eq!(ct.busy(), 100 + 10 + 50 + 3 + 2 + 1);
    }

    #[test]
    fn test_load_average_serde_roundtrip() {
        let load = LoadAverage {
            one: 1.5,
            five: 2.0,
            fifteen: 0.8,
        };
        let json = serde_json::to_string(&load).unwrap();
        let parsed: LoadAverage = serde_json::from_str(&json).unwrap();
        assert!((parsed.one - 1.5).abs() < f64::EPSILON);
        assert!((parsed.five - 2.0).abs() < f64::EPSILON);
        assert!((parsed.fifteen - 0.8).abs() < f64::EPSILON);
    }

    #[test]
    fn test_uptime_string_formatting() {
        let stats = SystemStats {
            load_average: None,
            uptime_seconds: Some(90061),
            idle_seconds: None,
            boot_time: None,
            num_cpus: 4,
            total_processes: Some(100),
            running_processes: Some(5),
            cpu_time: None,
            vm_stats: None,
            hostname: None,
            kernel_version: None,
        };
        assert_eq!(stats.uptime_string(), "1 days, 01:01");
    }

    #[test]
    fn test_uptime_string_no_days() {
        let stats = SystemStats {
            load_average: None,
            uptime_seconds: Some(3661),
            idle_seconds: None,
            boot_time: None,
            num_cpus: 4,
            total_processes: Some(100),
            running_processes: Some(5),
            cpu_time: None,
            vm_stats: None,
            hostname: None,
            kernel_version: None,
        };
        assert_eq!(stats.uptime_string(), "01:01:01");
    }

    #[test]
    fn test_load_string_none() {
        let stats = SystemStats {
            load_average: None,
            uptime_seconds: None,
            idle_seconds: None,
            boot_time: None,
            num_cpus: 4,
            total_processes: None,
            running_processes: None,
            cpu_time: None,
            vm_stats: None,
            hostname: None,
            kernel_version: None,
        };
        assert_eq!(stats.load_string(), "N/A");
    }

    #[test]
    fn test_load_string_present() {
        let stats = SystemStats {
            load_average: Some(LoadAverage {
                one: 1.5,
                five: 2.0,
                fifteen: 0.8,
            }),
            uptime_seconds: None,
            idle_seconds: None,
            boot_time: None,
            num_cpus: 4,
            total_processes: None,
            running_processes: None,
            cpu_time: None,
            vm_stats: None,
            hostname: None,
            kernel_version: None,
        };
        assert_eq!(stats.load_string(), "1.50, 2.00, 0.80");
    }
}
