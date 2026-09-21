//! Intel GPU monitoring via sysfs (i915/xe drivers)
//!
//! This module provides Intel GPU monitoring through sysfs on Linux.
//! It supports both integrated (iGPU) and discrete (Arc) GPUs using the i915 and xe drivers.
//!
//! # Monitored Metrics
//!
//! - Device information (name, PCI address, driver version)
//! - GPU utilization (render and compute engines)
//! - Memory usage (for discrete GPUs)
//! - Frequencies (current, min, max, boost)
//! - Power consumption and limits
//! - Temperatures
//!
//! # Implementation Notes
//!
//! Intel GPUs expose information through:
//! - `/sys/class/drm/card*/device/` - Device information
//! - `/sys/class/drm/card*/gt/` - GT (Graphics Technology) metrics
//! - `/sys/kernel/debug/dri/*/i915_*` - Debug info (requires root)
//! - hwmon subsystem for temperature and power
//!
//! Unlike AMD and NVIDIA, Intel doesn't have a comprehensive library like ROCm SMI or NVML,
//! so we rely on sysfs attributes exposed by the kernel driver.

use super::traits::{
    Clocks, Device, Error, FanSpeed, GpuProcess, Memory, PciInfo, Power, Temperature,
    TemperatureThresholds, Utilization, Vendor,
};
use std::fs;
use std::path::PathBuf;

/// Intel GPU device
pub struct IntelGpu {
    index: u32,
    #[allow(dead_code)]
    card_path: PathBuf,
    device_path: PathBuf,
    gt_path: Option<PathBuf>, // Graphics Technology path (xe driver)
}

impl IntelGpu {
    /// Create a new Intel GPU instance
    pub fn new(index: u32, card_path: PathBuf) -> Result<Self, Error> {
        let device_path = card_path.join("device");
        if !device_path.exists() {
            return Err(Error::InitializationFailed(
                "Device path does not exist".to_string(),
            ));
        }

        // Check for xe driver GT path (newer interface)
        let gt_path = card_path.join("gt");
        let gt_path = if gt_path.exists() {
            Some(gt_path)
        } else {
            None
        };

        Ok(Self {
            index,
            card_path,
            device_path,
            gt_path,
        })
    }

    /// Read a sysfs attribute as string
    fn read_sysfs_string(&self, attr: &str) -> Option<String> {
        fs::read_to_string(self.device_path.join(attr))
            .ok()
            .map(|s| s.trim().to_string())
    }

    /// Read a sysfs attribute as u64
    #[allow(dead_code)]
    fn read_sysfs_u64(&self, attr: &str) -> Option<u64> {
        self.read_sysfs_string(attr)?.parse::<u64>().ok()
    }

    /// Read GT sysfs attribute (xe driver)
    fn read_gt_string(&self, attr: &str) -> Option<String> {
        let gt_path = self.gt_path.as_ref()?;

        // Try gt0 first (most common)
        if let Ok(content) = fs::read_to_string(gt_path.join("gt0").join(attr)) {
            return Some(content.trim().to_string());
        }

        // Try gt_path directly
        fs::read_to_string(gt_path.join(attr))
            .ok()
            .map(|s| s.trim().to_string())
    }

    /// Read GT sysfs attribute as u64
    fn read_gt_u64(&self, attr: &str) -> Option<u64> {
        self.read_gt_string(attr)?.parse::<u64>().ok()
    }

    /// Read hwmon temperature sensor
    fn read_hwmon_temp(&self, hwmon_path: &std::path::Path, sensor: &str) -> Option<f32> {
        fs::read_to_string(hwmon_path.join(sensor))
            .ok()
            .and_then(|s| s.trim().parse::<i32>().ok())
            .map(|millidegrees| millidegrees as f32 / 1000.0)
    }

    fn get_temperature_thresholds(
        &self,
        hwmon_path: &std::path::Path,
    ) -> Option<TemperatureThresholds> {
        // Intel GPUs expose critical thresholds via hwmon
        // temp1_crit = GPU critical temperature
        // temp1_max = Max operating temperature

        let critical = self.read_hwmon_temp(hwmon_path, "temp1_crit");
        let max_temp = self.read_hwmon_temp(hwmon_path, "temp1_max");

        // Use max_temp as shutdown if critical not available
        let shutdown = critical.or(max_temp);

        // Intel doesn't expose slowdown directly
        // Estimate as 85% of critical/max if available
        let slowdown = critical.or(max_temp).map(|c| c * 0.85);

        if critical.is_some() || shutdown.is_some() || slowdown.is_some() {
            Some(TemperatureThresholds {
                slowdown,
                shutdown,
                critical,
                memory_critical: None,
            })
        } else {
            None
        }
    }

    /// Get driver name (i915 or xe)
    fn get_driver_name(&self) -> Option<String> {
        // Read driver link
        if let Ok(link) = fs::read_link(self.device_path.join("driver")) {
            if let Some(name) = link.file_name() {
                return Some(name.to_string_lossy().to_string());
            }
        }
        None
    }
}

impl Device for IntelGpu {
    fn vendor(&self) -> Vendor {
        Vendor::Intel
    }

    fn index(&self) -> u32 {
        self.index
    }

    fn name(&self) -> Result<String, Error> {
        // Try to read product name
        if let Some(name) = self.read_sysfs_string("product_name") {
            return Ok(name);
        }

        // Try uevent for model info
        if let Ok(uevent) = fs::read_to_string(self.device_path.join("uevent")) {
            for line in uevent.lines() {
                if line.starts_with("DRIVER=") {
                    let driver = line.strip_prefix("DRIVER=").unwrap_or("");
                    return Ok(format!("Intel {} GPU", driver));
                }
            }
        }

        // Read device ID
        if let Some(device_id) = self.read_sysfs_string("device") {
            // Map common Intel device IDs to names
            let name = match device_id.as_str() {
                "0x4680" | "0x4682" | "0x4688" | "0x468a" | "0x468b" => "Intel Arc A770",
                "0x4690" | "0x4692" | "0x4693" => "Intel Arc A750",
                "0x56a0" | "0x56a1" | "0x56a2" => "Intel Arc A580",
                "0x56a5" | "0x56a6" => "Intel Arc A380",
                "0x46a0" | "0x46a1" | "0x46a2" | "0x46a3" | "0x46a6" | "0x46a8" | "0x46aa"
                | "0x462a" | "0x4626" | "0x4628" => "Intel Iris Xe",
                _ => "Intel GPU",
            };
            return Ok(format!("{} ({})", name, device_id));
        }

        Ok(format!("Intel GPU #{}", self.index))
    }

    fn uuid(&self) -> Result<String, Error> {
        // Intel GPUs don't expose UUIDs via sysfs
        // Generate one from vendor, device, and subsystem IDs
        let vendor = self.read_sysfs_string("vendor").unwrap_or_default();
        let device = self.read_sysfs_string("device").unwrap_or_default();
        let subsystem_vendor = self
            .read_sysfs_string("subsystem_vendor")
            .unwrap_or_default();
        let subsystem_device = self
            .read_sysfs_string("subsystem_device")
            .unwrap_or_default();

        Ok(format!(
            "GPU-{}-{}-{}-{}",
            vendor, device, subsystem_vendor, subsystem_device
        ))
    }

    fn pci_info(&self) -> Result<PciInfo, Error> {
        // Parse PCI address from sysfs path
        let link = fs::read_link(&self.device_path)
            .map_err(|e| Error::QueryFailed(format!("Failed to read device link: {}", e)))?;

        let path_str = link.to_string_lossy();

        // Find PCI address pattern (0000:00:00.0)
        for component in path_str.split('/').rev() {
            let parts: Vec<&str> = component.split(':').collect();
            if parts.len() == 3 {
                let domain_bus = parts[0];
                let device_func = parts[2].split('.').collect::<Vec<&str>>();

                if domain_bus.len() == 7 && device_func.len() == 2 {
                    if let (Ok(domain), Ok(bus), Ok(device), Ok(function)) = (
                        u16::from_str_radix(&domain_bus[0..4], 16),
                        u8::from_str_radix(&domain_bus[5..7], 16),
                        u8::from_str_radix(parts[1], 16),
                        u8::from_str_radix(device_func[1], 16),
                    ) {
                        let bus_id =
                            format!("{:04x}:{:02x}:{:02x}.{}", domain, bus, device, function);
                        return Ok(PciInfo {
                            domain: domain as u32,
                            bus,
                            device,
                            function,
                            bus_id,
                            pcie_generation: None,
                            pcie_link_width: None,
                        });
                    }
                }
            }
        }

        Err(Error::QueryFailed("Could not parse PCI info".to_string()))
    }

    fn driver_version(&self) -> Result<String, Error> {
        // Try to read module version
        if let Some(driver) = self.get_driver_name() {
            if let Ok(version) = fs::read_to_string(format!("/sys/module/{}/version", driver)) {
                return Ok(version.trim().to_string());
            }
            return Ok(driver);
        }

        Ok("i915".to_string())
    }

    fn temperature(&self) -> Result<Temperature, Error> {
        // Find hwmon directory
        let hwmon_path = self.device_path.join("hwmon");
        if !hwmon_path.exists() {
            return Ok(Temperature {
                edge: None,
                junction: None,
                memory: None,
                hotspot: None,
                vr_gfx: None,
                vr_soc: None,
                vr_mem: None,
                hbm: None,
                thresholds: None,
            });
        }

        let hwmon_dirs: Vec<_> = fs::read_dir(&hwmon_path)
            .map_err(|e| Error::QueryFailed(format!("Failed to read hwmon dir: {}", e)))?
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("hwmon"))
            .collect();

        if hwmon_dirs.is_empty() {
            return Ok(Temperature {
                edge: None,
                junction: None,
                memory: None,
                hotspot: None,
                vr_gfx: None,
                vr_soc: None,
                vr_mem: None,
                hbm: None,
                thresholds: None,
            });
        }

        let hwmon = hwmon_dirs[0].path();

        // Intel GPUs typically expose GPU temperature on temp1
        let junction = self.read_hwmon_temp(&hwmon, "temp1_input");

        // Read temperature thresholds
        let thresholds = self.get_temperature_thresholds(&hwmon);

        Ok(Temperature {
            edge: None,
            junction,
            memory: None,
            hotspot: None,
            vr_gfx: None,
            vr_soc: None,
            vr_mem: None,
            hbm: None,
            thresholds,
        })
    }

    fn power(&self) -> Result<Power, Error> {
        // Find hwmon directory
        let hwmon_path = self.device_path.join("hwmon");
        if !hwmon_path.exists() {
            return Err(Error::NotSupported);
        }

        let hwmon_dirs: Vec<_> = fs::read_dir(&hwmon_path)
            .map_err(|e| Error::QueryFailed(format!("Failed to read hwmon dir: {}", e)))?
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().starts_with("hwmon"))
            .collect();

        if hwmon_dirs.is_empty() {
            return Err(Error::NotSupported);
        }

        let hwmon = hwmon_dirs[0].path();

        // Read power values (in microwatts, convert to watts)
        // A hwmon node that is missing or unparseable reports nothing. It
        // used to report 0 W, which on a power rail is a specific claim.
        let read_power = |sensor: &str| -> Option<f32> {
            fs::read_to_string(hwmon.join(sensor))
                .ok()
                .and_then(|s| s.trim().parse::<u64>().ok())
                .map(|microwatts| microwatts as f32 / 1_000_000.0)
        };

        let current = read_power("power1_input");
        let limit = read_power("power1_cap");
        let max_limit = read_power("power1_cap_max");

        // See the AMD reader for `average`. `min_limit` was a hardcoded
        // `0.0`: this path reads no minimum at all, and 0 W is not one.
        Ok(Power {
            current,
            average: current,
            limit,
            default_limit: limit,
            min_limit: None,
            max_limit,
            enforced_limit: limit,
        })
    }

    fn clocks(&self) -> Result<Clocks, Error> {
        // Try xe driver interface first
        if let Some(freq) = self.read_gt_u64("freq0/act_freq") {
            return Ok(Clocks {
                graphics: Some((freq / 1_000_000) as u32), // Convert Hz to MHz
                // Not exposed by the xe driver. `0` said "this card's memory is
                // clocked at zero", which no running card is.
                memory: None,
                sm: None,
                video: None,
            });
        }

        // Try i915 driver interface
        if let Some(freq_str) = self.read_sysfs_string("gt_cur_freq_mhz") {
            if let Ok(freq) = freq_str.parse::<u32>() {
                return Ok(Clocks {
                    graphics: Some(freq),
                    // Not exposed by i915 either. See above.
                    memory: None,
                    sm: None,
                    video: None,
                });
            }
        }

        // Neither driver interface answered. This used to return `Ok` with
        // every field zero -- a success carrying a fabricated reading, which a
        // caller cannot tell from an idle card.
        Ok(Clocks {
            graphics: None,
            memory: None,
            sm: None,
            video: None,
        })
    }

    fn utilization(&self) -> Result<Utilization, Error> {
        // Intel does not expose utilization via sysfs; it would require
        // i915_engine_info debugfs or performance counters.
        //
        // This returned `0.0` for both, which reported every Intel GPU as
        // permanently idle rather than as unmeasured — a reading no code here
        // ever took, in the field a capacity planner acts on.
        Ok(Utilization {
            gpu: None,
            memory: None,
            encoder: None,
            decoder: None,
            jpeg: None,
            ofa: None,
        })
    }

    fn memory(&self) -> Result<Memory, Error> {
        // For discrete GPUs, try to read LMEM (Local Memory)
        // This is available on Arc GPUs with xe driver
        if let Some(total) = self.read_gt_u64("mem_info/total") {
            let used = self.read_gt_u64("mem_info/used");
            // Derived only when both operands exist: a free figure computed
            // against an absent `used` is arithmetic on a guess.
            let free = used.map(|u| total.saturating_sub(u));

            return Ok(Memory {
                total: Some(total),
                used,
                free,
                bar1_total: None,
                bar1_used: None,
            });
        }

        // For integrated GPUs memory is shared with the system and is not
        // readable here (debugfs, needing root). This returned zeros, which
        // reported an integrated GPU as having no memory at all rather than as
        // having memory nobody asked.
        Ok(Memory {
            total: None,
            used: None,
            free: None,
            bar1_total: None,
            bar1_used: None,
        })
    }

    fn fan_speed(&self) -> Result<Option<FanSpeed>, Error> {
        // Most Intel GPUs don't have fans (integrated)
        // Discrete Arc cards might, check hwmon
        let hwmon_path = self.device_path.join("hwmon");
        if !hwmon_path.exists() {
            return Ok(None);
        }

        let hwmon_dir = fs::read_dir(&hwmon_path)
            .ok()
            .and_then(|rd| rd.filter_map(|e| e.ok()).nth(0))
            .map(|e| e.path());

        if let Some(hwmon) = hwmon_dir {
            // Try to read fan percentage from PWM first (nvtop parity)
            // PWM is 0-255, pwm1_max defines the maximum value
            if let Some(pwm) = fs::read_to_string(hwmon.join("pwm1"))
                .ok()
                .and_then(|s| s.trim().parse::<u32>().ok())
            {
                let pwm_max = fs::read_to_string(hwmon.join("pwm1_max"))
                    .ok()
                    .and_then(|s| s.trim().parse::<u32>().ok())
                    .unwrap_or(255); // Default to 255 if pwm1_max not available

                let percentage = (pwm * 100) / pwm_max;
                return Ok(Some(FanSpeed::Percent(percentage)));
            }

            // Fallback: Read fan speed in RPM
            if let Some(rpm) = fs::read_to_string(hwmon.join("fan1_input"))
                .ok()
                .and_then(|s| s.trim().parse::<u32>().ok())
            {
                return Ok(Some(FanSpeed::Rpm(rpm)));
            }
        }

        Ok(None)
    }

    fn performance_state(&self) -> Result<Option<String>, Error> {
        // Read boost status
        if let Some(boost) = self.read_sysfs_string("gt_boost_freq_mhz") {
            return Ok(Some(format!("Boost: {} MHz", boost)));
        }

        Ok(None)
    }

    fn processes(&self) -> Result<Vec<Box<dyn GpuProcess>>, Error> {
        // Would need to parse /proc/*/fdinfo for DRM clients
        // This is complex and requires matching file descriptors
        Ok(Vec::new())
    }
}

/// Enumerate all Intel GPUs in the system
pub fn enumerate() -> Result<Vec<Box<dyn Device>>, Error> {
    let mut devices: Vec<Box<dyn Device>> = Vec::new();

    // Scan /sys/class/drm for Intel GPU devices
    let drm_path = std::path::Path::new("/sys/class/drm");
    if !drm_path.exists() {
        return Ok(devices);
    }

    let entries = fs::read_dir(drm_path).map_err(|e| {
        Error::InitializationFailed(format!("Failed to read /sys/class/drm: {}", e))
    })?;

    for entry in entries.filter_map(|e| e.ok()) {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Look for card devices (card0, card1, etc) - skip render nodes
        if !name_str.starts_with("card") || name_str.contains('-') {
            continue;
        }

        // Extract card number
        let index = name_str.trim_start_matches("card").parse::<u32>().ok();

        let Some(index) = index else {
            continue;
        };

        let card_path = entry.path();
        let device_path = card_path.join("device");

        // Check if this is an Intel GPU (vendor ID 0x8086)
        if let Ok(vendor_id) = fs::read_to_string(device_path.join("vendor")) {
            if vendor_id.trim() == "0x8086" {
                // Verify it's a GPU by checking class (0x03xxxx for display)
                if let Ok(class) = fs::read_to_string(device_path.join("class")) {
                    if class.trim().starts_with("0x03") {
                        match IntelGpu::new(index, card_path) {
                            Ok(gpu) => devices.push(Box::new(gpu)),
                            Err(_) => continue,
                        }
                    }
                }
            }
        }
    }

    if devices.is_empty() {
        return Err(Error::NoDevicesFound);
    }

    Ok(devices)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intel_vendor() {
        // Verify Intel vendor is properly set
        let vendor = Vendor::Intel;
        assert_eq!(format!("{}", vendor), "Intel");
    }
}
