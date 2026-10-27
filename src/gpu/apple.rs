//! Apple GPU monitoring (M1/M2/M3/M4 integrated GPUs)
//!
//! This module provides GPU monitoring for Apple Silicon using powermetrics
//! and Metal Performance Shaders framework.

use super::*;
use crate::error::Error;
use crate::silicon::apple::AppleGpuCounters;

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
            .env("LC_ALL", "C")
            .output()
            .map_err(|e| Error::CommandExecutionFailed(format!("system_profiler: {}", e)))?;

        if !output.status.success() {
            return Err(Error::CommandExecutionFailed(
                "system_profiler could not enumerate displays".into(),
            ));
        }
        Ok(Self::parse_inventory(&String::from_utf8_lossy(
            &output.stdout,
        )))
    }

    fn parse_inventory(text: &str) -> Vec<Self> {
        let mut gpus: Vec<Self> = Vec::new();
        let mut current = None;
        for line in text.lines() {
            let Some((key, value)) = line.trim().split_once(':') else {
                continue;
            };
            match key {
                "Chipset Model" => {
                    current = None;
                    if value.trim().starts_with("Apple ") {
                        current = Some(gpus.len());
                        gpus.push(Self::new(
                            gpus.len() as u32,
                            format!("{} GPU", value.trim()),
                            None,
                        ));
                    }
                }
                "Total Number of Cores" => {
                    if let Some(index) = current {
                        gpus[index].cores = value.trim().parse::<u32>().ok().filter(|n| *n > 0);
                    }
                }
                _ => {}
            }
        }
        gpus
    }

    /// This GPU's entry among IOKit's accelerators: the one whose `model` is
    /// this GPU's chipset, or the only one when this is the only Apple GPU.
    fn counters_in<'a>(&self, gpus: &'a [AppleGpuCounters]) -> Option<&'a AppleGpuCounters> {
        let chipset = self.name.strip_suffix(" GPU").unwrap_or(&self.name);
        gpus.iter()
            .find(|gpu| gpu.model.as_deref() == Some(chipset))
            .or_else(|| (self.index == 0 && gpus.len() == 1).then(|| &gpus[0]))
    }

    /// The live figures IOKit's counters carry.
    fn dynamic_from(counters: &AppleGpuCounters) -> GpuDynamicInfo {
        let mut vendor_specific = Vec::new();
        if let Some(tiler) = counters.tiler_utilization {
            vendor_specific.push(("tiler".to_string(), tiler));
        }
        GpuDynamicInfo {
            utilization: counters.device_utilization,
            // The GPU has no memory of its own; it uses the SoC's unified
            // memory, and this is how much of it. There is no GPU total to
            // divide by -- the share the GPU may take is the OS's to decide --
            // so neither free nor a percentage is reported.
            memory: GpuMemory {
                used: counters.in_use_system_memory,
                ..GpuMemory::unreported()
            },
            clocks: GpuClocks {
                graphics: None,     // Needs `powermetrics`, which needs root
                graphics_max: None, // Apple does not publish a GPU clock ceiling
                memory: None,       // Unified memory, no separate memory clock
                memory_max: None,
                sm: None,
                video: None,
            },
            power: GpuPower {
                draw: None, // Needs `powermetrics`, which needs root
                limit: None,
                default_limit: None,
                // A real draw divided by a guessed ceiling is not a measurement.
                // This used to divide by a `max_power` picked from a core-count
                // bucket, so the percentage looked derived from a measurement
                // while its denominator was invented.
                usage_percent: None,
            },
            thermal: GpuThermal {
                temperature: None,
                // Apple publishes no GPU thermal limits. These were 100/110 with
                // a "conservative estimate" comment, which reached callers as
                // read thresholds.
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
                graphics: counters.renderer_utilization,
                compute: None,
                encoder: None,
                decoder: None,
                copy: None,
                vendor_specific,
            },
            processes: Vec::new(),
        }
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
            integrated: Some(true), // Apple Silicon GPUs are always integrated
        })
    }

    fn dynamic_info(&self) -> Result<GpuDynamicInfo, crate::Error> {
        // IOKit's counters for this GPU, which need no root. This read only
        // `powermetrics`, which does, so without root an Apple GPU had no live
        // figure at all.
        let gpus = crate::silicon::apple::read_gpu_counters()
            .map_err(crate::Error::CommandExecutionFailed)?;
        let counters = self.counters_in(&gpus).ok_or_else(|| {
            crate::Error::DeviceNotFound(format!("IOKit lists no IOAccelerator for {}", self.name))
        })?;
        Ok(Self::dynamic_from(counters))
    }

    fn processes(&self) -> Result<Vec<GpuProcess>, crate::Error> {
        // Process tracking is not available via powermetrics, so no list was
        // read. This used to return an empty list, which reads as "none".
        Err(crate::Error::NotSupported(
            concat!(
                "GPU process lists are not read on macOS; ",
                "powermetrics reports no per-process GPU use"
            )
            .to_string(),
        ))
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

#[cfg(test)]
mod inventory_tests {
    use super::AppleGpu;
    use crate::silicon::apple::AppleGpuCounters;

    /// IOKit's counters become the GPU's live figures, matched to the GPU by
    /// chipset, and what they do not carry stays unread.
    #[test]
    fn iokit_counters_are_this_gpus_live_figures() {
        let gpu = AppleGpu::new(0, "Apple M1 GPU".into(), Some(8));
        let other = AppleGpuCounters {
            model: Some("Apple Paravirtual device".into()),
            ..Default::default()
        };
        let mine = AppleGpuCounters {
            model: Some("Apple M1".into()),
            device_utilization: Some(12),
            renderer_utilization: Some(11),
            tiler_utilization: Some(7),
            in_use_system_memory: Some(423_624_704),
            ..Default::default()
        };
        let gpus = [other, mine.clone()];
        assert_eq!(gpu.counters_in(&gpus), Some(&mine));
        let dynamic = AppleGpu::dynamic_from(&mine);
        assert_eq!(dynamic.utilization, Some(12));
        assert_eq!(dynamic.engines.graphics, Some(11));
        assert_eq!(
            dynamic.engines.vendor_specific,
            vec![("tiler".to_string(), 7)]
        );
        assert_eq!(dynamic.memory.used, Some(423_624_704));
        assert_eq!(
            dynamic.memory.total, None,
            "unified memory has no GPU total"
        );
        assert_eq!(dynamic.memory.utilization, None);
        assert_eq!(dynamic.clocks.graphics, None);
        assert_eq!(dynamic.power.draw, None);

        // Two GPUs neither of which is this one: no match, not a guess.
        let strangers = [AppleGpuCounters::default(), AppleGpuCounters::default()];
        assert_eq!(gpu.counters_in(&strangers), None);
        // The only accelerator, for the only Apple GPU, is the same device.
        let lone = [AppleGpuCounters::default()];
        assert!(gpu.counters_in(&lone).is_some());
    }

    #[test]
    fn apple_inventory_uses_enumerated_devices_and_keeps_unknown_core_counts() {
        let gpus = AppleGpu::parse_inventory("Chipset Model: Apple M1\nTotal Number of Cores: 8\nChipset Model: Intel Iris\nTotal Number of Cores: 24\nChipset Model: Apple M4\n");
        assert_eq!(gpus.len(), 2);
        assert_eq!(gpus[0].name, "Apple M1 GPU");
        assert_eq!(gpus[0].cores, Some(8));
        assert_eq!(gpus[1].cores, None);
        assert!(AppleGpu::parse_inventory("").is_empty());
        assert!(AppleGpu::parse_inventory("Chipset Model: Intel Iris\n").is_empty());
    }
}
