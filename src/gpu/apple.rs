//! Apple GPU monitoring (M1/M2/M3/M4 integrated GPUs)
//!
//! This module provides GPU monitoring for Apple Silicon using powermetrics
//! and Metal Performance Shaders framework.

use super::*;
use crate::error::Error;

/// Apple GPU implementation
pub struct AppleGpu {
    index: u32,
    name: String,
    /// GPU core count, where `system_profiler` reported one.
    ///
    /// `None` rather than a default. This was `u32` seeded with `8` and parsed
    /// with `unwrap_or(8)`, and it reached `GpuStaticInfo::shader_cores` as
    /// `Some(8)` -- a sentinel wrapped in the very type that exists to say
    /// whether anything was read. `shader_cores` is `Option<u32>` precisely so
    /// that an unread count can be `None`, and the reader filled it in anyway.
    cores: Option<u32>,
}

impl AppleGpu {
    /// Create new Apple GPU instance
    ///
    /// Formerly also derived a `max_frequency` and `max_power` from a core-count
    /// bucket (1400–1550 MHz, 20–130 W). Neither was read from the hardware, and both
    /// escaped as though they were: `max_frequency` was reported as the GPU's
    /// `graphics_max` clock, and `max_power` was the denominator of a power
    /// `usage_percent`. Apple exposes neither figure, so both are now `None`.
    pub fn new(index: u32, name: String, cores: Option<u32>) -> Self {
        Self { index, name, cores }
    }

    /// Detect Apple GPUs
    pub fn detect_gpus() -> Result<Vec<AppleGpu>, Error> {
        // Use system_profiler to get GPU info
        let output = std::process::Command::new("system_profiler")
            .args(["-detailLevel", "basic", "SPDisplaysDataType"])
            .output()
            .map_err(|e| Error::CommandExecutionFailed(format!("system_profiler: {}", e)))?;

        let text = String::from_utf8_lossy(&output.stdout);
        let mut gpus = Vec::new();

        // `None` until `system_profiler` states a count.
        //
        // This was `let mut cores = 8; // Default`, parsed with
        // `unwrap_or(8)`. Eight is the GPU core count of a base M1 and M2, so
        // a machine whose `system_profiler` output lacked the line -- or whose
        // count failed to parse -- was reported as one of those, and an M3 Max
        // with forty cores would have been reported as having eight.
        let mut cores: Option<u32> = None;

        for line in text.lines() {
            if line.contains("Total Number of Cores") {
                if let Some(cores_str) = line.split(':').nth(1) {
                    cores = cores_str.trim().parse().ok();
                }
            }
        }

        // Get SOC name via sysctl
        let output = std::process::Command::new("sysctl")
            .args(["-n", "machdep.cpu.brand_string"])
            .output()
            .map_err(|e| Error::CommandExecutionFailed(format!("sysctl: {}", e)))?;

        let soc_name = String::from_utf8_lossy(&output.stdout).trim().to_string();

        // Create GPU instance
        gpus.push(AppleGpu::new(0, format!("{} GPU", soc_name), cores));

        Ok(gpus)
    }
}

#[cfg(feature = "apple")]
impl Gpu for AppleGpu {
    fn vendor(&self) -> GpuVendor {
        GpuVendor::Apple
    }

    fn index(&self) -> usize {
        self.index as usize
    }

    fn name(&self) -> Result<String, crate::Error> {
        Ok(self.name.clone())
    }

    fn static_info(&self) -> Result<GpuStaticInfo, crate::Error> {
        Ok(GpuStaticInfo {
            index: self.index as usize,
            vendor: GpuVendor::Apple,
            name: self.name.clone(),
            pci_bus_id: None,
            uuid: None,
            vbios_version: None,
            driver_version: None,
            compute_capability: None,
            shader_cores: self.cores,
            l2_cache: None,
            num_engines: None,
            integrated: true, // Apple Silicon GPUs are always integrated
        })
    }

    fn dynamic_info(&self) -> Result<GpuDynamicInfo, crate::Error> {
        // Get metrics from powermetrics via the Silicon monitor
        use crate::silicon::apple::AppleSiliconMonitor;

        let monitor = AppleSiliconMonitor::new()?;
        let data = monitor.parse_powermetrics()?;

        Ok(GpuDynamicInfo {
            utilization: Some(data.gpu_active),
            // `powermetrics` reports no GPU memory figures at all, and Apple
            // Silicon has no separate VRAM to report -- the GPU shares system
            // memory. The comment here used to say "Not available from
            // powermetrics" on a line assigning `0`, which is the whole defect
            // in miniature: the code knew, and the type could not say.
            memory: GpuMemory::unreported(),
            clocks: GpuClocks {
                graphics: Some(data.gpu_freq_mhz),
                graphics_max: None, // Apple does not publish a GPU clock ceiling
                memory: None,       // Unified memory, no separate memory clock
                memory_max: None,
                sm: None,
                video: None,
            },
            power: GpuPower {
                draw: Some(data.gpu_power_mw),
                limit: None, // Not available
                default_limit: None,
                // A real draw divided by a guessed ceiling is not a measurement. This
                // used to divide by a `max_power` picked from a core-count bucket, so
                // the percentage looked derived from a measurement while its
                // denominator was invented. `limit` above is already `None` for the
                // same reason; reporting a percentage of a limit we admit we do not
                // know contradicted it.
                usage_percent: None,
            },
            thermal: GpuThermal {
                temperature: None, // Not available from powermetrics
                // Apple publishes no GPU thermal limits and powermetrics reports
                // none. These were 100/110 with a "conservative estimate" comment,
                // which reached callers as read thresholds — `ai_api` forwards them
                // verbatim as `max_temperature_c` and `critical_temperature_c`. Every
                // other backend reports `None` when the limit is unknown; AMD reads
                // the real value from hwmon.
                max_temperature: None,
                critical_temperature: None,
                fan_speed: None,
                fan_rpm: None,
            },
            pcie: PcieLinkInfo {
                current_gen: None, // Integrated GPU, no PCIe
                current_width: None,
                max_gen: None,
                max_width: None,
                current_speed: None,
                max_speed: None,
                tx_throughput: None,
                rx_throughput: None,
            },
            engines: GpuEngines {
                graphics: Some(data.gpu_active),
                compute: Some(data.gpu_active), // Same as graphics on Apple Silicon
                encoder: None,                  // Not available
                decoder: None,                  // Not available
                copy: None,                     // Not available
                vendor_specific: Vec::new(),
            },
            processes: Vec::new(), // Not available from powermetrics
        })
    }

    fn processes(&self) -> Result<Vec<GpuProcess>, crate::Error> {
        // Process tracking not available via powermetrics
        Ok(Vec::new())
    }

    fn kill_process(&self, _pid: u32) -> Result<(), crate::Error> {
        Err(crate::Error::Unsupported(
            "Process killing not supported on Apple GPUs".to_string(),
        ))
    }

    fn set_power_limit(&mut self, _limit_mw: u32) -> Result<(), crate::Error> {
        Err(crate::Error::Unsupported(
            "Power limit control not supported on Apple GPUs".to_string(),
        ))
    }
}
