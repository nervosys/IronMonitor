// Windows-specific hardware monitoring implementations
//
// This module provides native Windows implementations for reading hardware sensors
// including GPU temperatures (NVML), and potentially Super I/O chip sensors.

use super::{HwSensor, HwSensorType, HwType};

#[cfg(feature = "nvidia")]
use nvml_wrapper::{enum_wrappers::device::TemperatureSensor, Nvml};

/// Read all GPU temperatures
pub fn read_gpu_temperatures() -> Vec<HwSensor> {
    #[allow(unused_mut)]
    let mut sensors = Vec::new();

    #[cfg(feature = "nvidia")]
    {
        sensors.extend(read_nvidia_gpu_temps());
    }

    // AMD and Intel GPU temperatures are collected via their respective
    // GPU backends (WmiAmdGpu/WmiIntelGpu) using performance counters
    // and WMI thermal zones. See gpu/amd.rs and gpu/intel.rs.

    sensors
}

#[cfg(feature = "nvidia")]
fn read_nvidia_gpu_temps() -> Vec<HwSensor> {
    let mut sensors = Vec::new();

    if let Ok(nvml) = Nvml::init() {
        if let Ok(count) = nvml.device_count() {
            for i in 0..count {
                if let Ok(device) = nvml.device_by_index(i) {
                    let name = device.name().unwrap_or_else(|_| format!("GPU {}", i));

                    // GPU Core temperature
                    if let Ok(temp) = device.temperature(TemperatureSensor::Gpu) {
                        sensors.push(HwSensor {
                            name: format!("{} Core", name),
                            value: temp as f32,
                            min: None,
                            max: None,
                            sensor_type: HwSensorType::Temperature,
                            hardware_type: HwType::Gpu,
                        });
                    }

                    // Memory temperature (if available)
                    // Note: Not all GPUs expose memory temperature
                    // NVML 11+ has nvmlDeviceGetMemoryInfo_v2 but not direct temp

                    // Hotspot temperature (available on newer GPUs)
                    // This would require checking NVML version and device capabilities
                }
            }
        }
    }

    sensors
}

/// Read GPU fan speeds
#[cfg(feature = "nvidia")]
pub fn read_nvidia_fan_speeds() -> Vec<HwSensor> {
    let mut sensors = Vec::new();

    if let Ok(nvml) = Nvml::init() {
        if let Ok(count) = nvml.device_count() {
            for i in 0..count {
                if let Ok(device) = nvml.device_by_index(i) {
                    let name = device.name().unwrap_or_else(|_| format!("GPU {}", i));

                    // Fan speed as percentage
                    if let Ok(fan_speed) = device.fan_speed(0) {
                        sensors.push(HwSensor {
                            name: format!("{} Fan", name),
                            value: fan_speed as f32,
                            min: Some(0.0),
                            max: Some(100.0),
                            sensor_type: HwSensorType::Fan,
                            hardware_type: HwType::Gpu,
                        });
                    }

                    // Some GPUs have multiple fans
                    if let Ok(num_fans) = device.num_fans() {
                        for fan_idx in 1..num_fans {
                            if let Ok(fan_speed) = device.fan_speed(fan_idx) {
                                sensors.push(HwSensor {
                                    name: format!("{} Fan {}", name, fan_idx + 1),
                                    value: fan_speed as f32,
                                    min: Some(0.0),
                                    max: Some(100.0),
                                    sensor_type: HwSensorType::Fan,
                                    hardware_type: HwType::Gpu,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    sensors
}

/// Read GPU power consumption
#[cfg(feature = "nvidia")]
pub fn read_nvidia_power() -> Vec<HwSensor> {
    let mut sensors = Vec::new();

    if let Ok(nvml) = Nvml::init() {
        if let Ok(count) = nvml.device_count() {
            for i in 0..count {
                if let Ok(device) = nvml.device_by_index(i) {
                    let name = device.name().unwrap_or_else(|_| format!("GPU {}", i));

                    // Power usage in milliwatts
                    if let Ok(power_mw) = device.power_usage() {
                        sensors.push(HwSensor {
                            name: format!("{} Power", name),
                            value: power_mw as f32 / 1000.0, // Convert to watts
                            min: None,
                            max: device
                                .power_management_limit()
                                .ok()
                                .map(|p| p as f32 / 1000.0),
                            sensor_type: HwSensorType::Power,
                            hardware_type: HwType::Gpu,
                        });
                    }
                }
            }
        }
    }

    sensors
}

/// Read GPU clock speeds
#[cfg(feature = "nvidia")]
pub fn read_nvidia_clocks() -> Vec<HwSensor> {
    use nvml_wrapper::enum_wrappers::device::Clock;

    let mut sensors = Vec::new();

    if let Ok(nvml) = Nvml::init() {
        if let Ok(count) = nvml.device_count() {
            for i in 0..count {
                if let Ok(device) = nvml.device_by_index(i) {
                    let name = device.name().unwrap_or_else(|_| format!("GPU {}", i));

                    // Graphics clock
                    if let Ok(clock) = device.clock_info(Clock::Graphics) {
                        sensors.push(HwSensor {
                            name: format!("{} Graphics Clock", name),
                            value: clock as f32,
                            min: None,
                            max: device
                                .max_clock_info(Clock::Graphics)
                                .ok()
                                .map(|c| c as f32),
                            sensor_type: HwSensorType::Clock,
                            hardware_type: HwType::Gpu,
                        });
                    }

                    // Memory clock
                    if let Ok(clock) = device.clock_info(Clock::Memory) {
                        sensors.push(HwSensor {
                            name: format!("{} Memory Clock", name),
                            value: clock as f32,
                            min: None,
                            max: device.max_clock_info(Clock::Memory).ok().map(|c| c as f32),
                            sensor_type: HwSensorType::Clock,
                            hardware_type: HwType::Gpu,
                        });
                    }

                    // SM clock
                    if let Ok(clock) = device.clock_info(Clock::SM) {
                        sensors.push(HwSensor {
                            name: format!("{} SM Clock", name),
                            value: clock as f32,
                            min: None,
                            max: device.max_clock_info(Clock::SM).ok().map(|c| c as f32),
                            sensor_type: HwSensorType::Clock,
                            hardware_type: HwType::Gpu,
                        });
                    }
                }
            }
        }
    }

    sensors
}

/// Read GPU utilization
#[cfg(feature = "nvidia")]
pub fn read_nvidia_utilization() -> Vec<HwSensor> {
    let mut sensors = Vec::new();

    if let Ok(nvml) = Nvml::init() {
        if let Ok(count) = nvml.device_count() {
            for i in 0..count {
                if let Ok(device) = nvml.device_by_index(i) {
                    let name = device.name().unwrap_or_else(|_| format!("GPU {}", i));

                    if let Ok(util) = device.utilization_rates() {
                        // GPU utilization
                        sensors.push(HwSensor {
                            name: format!("{} GPU Load", name),
                            value: util.gpu as f32,
                            min: Some(0.0),
                            max: Some(100.0),
                            sensor_type: HwSensorType::Load,
                            hardware_type: HwType::Gpu,
                        });

                        // Memory utilization
                        sensors.push(HwSensor {
                            name: format!("{} Memory Load", name),
                            value: util.memory as f32,
                            min: Some(0.0),
                            max: Some(100.0),
                            sensor_type: HwSensorType::Load,
                            hardware_type: HwType::Gpu,
                        });
                    }
                }
            }
        }
    }

    sensors
}

/// Aggregate all NVIDIA GPU sensors
#[cfg(feature = "nvidia")]
pub fn read_all_nvidia_sensors() -> Vec<HwSensor> {
    let mut sensors = Vec::new();

    sensors.extend(read_nvidia_gpu_temps());
    sensors.extend(read_nvidia_fan_speeds());
    sensors.extend(read_nvidia_power());
    sensors.extend(read_nvidia_clocks());
    sensors.extend(read_nvidia_utilization());

    sensors
}

#[cfg(not(feature = "nvidia"))]
pub fn read_all_nvidia_sensors() -> Vec<HwSensor> {
    Vec::new()
}

// =============================================================================
// Super I/O Chip Reading (requires elevated privileges or kernel driver)
// =============================================================================
//
// Reading motherboard sensors (voltage, fan RPM, etc.) typically requires
// direct I/O port access to Super I/O chips (ITE IT87xx, Nuvoton NCT67xx, etc.)
//
// On Windows, this requires either:
// 1. A kernel driver (like WinRing0 used by LibreHardwareMonitor)
// 2. Running as admin with specific privileges
//
// The I/O ports used are typically:
// - 0x2E/0x2F (primary Super I/O)
// - 0x4E/0x4F (secondary Super I/O)
//
// Since we can't directly access I/O ports from user mode without a driver,
// this functionality is limited. Consider using LibreHardwareMonitor's WMI
// interface when available, or implement a minimal kernel driver.

/// Super I/O chip sensors — always empty on Windows.
///
/// This is a hard platform limitation, not an unimplemented stub. Reading a Super
/// I/O chip means direct I/O port access (0x2E/0x2F, 0x4E/0x4F), which user-mode
/// code cannot perform on Windows. Tools that manage it ship a signed kernel driver
/// for the purpose — LibreHardwareMonitor bundles WinRing0.sys.
///
/// Shipping such a driver is out of scope: it is a well-known privilege-escalation
/// vector, and loading one would give ironmon far more kernel authority than a monitor
/// warrants. Use [`read_wmi_temperatures`] for what Windows does expose, or run
/// LibreHardwareMonitor alongside and read its WMI namespace.
pub fn read_superio_sensors() -> Vec<HwSensor> {
    // Without a kernel driver, we cannot read Super I/O chips
    // LibreHardwareMonitor bundles WinRing0.sys for this purpose
    Vec::new()
}

// =============================================================================
// Windows Management Instrumentation (WMI) fallbacks
// =============================================================================

/// Read any available Windows thermal zones via WMI
#[cfg(target_os = "windows")]
pub fn read_wmi_temperatures() -> Vec<HwSensor> {
    use std::collections::HashMap;
    use wmi::{COMLibrary, Variant, WMIConnection};

    let mut sensors = Vec::new();

    // MSAcpi_ThermalZoneTemperature is in the root\\WMI namespace
    if let Ok(com) = COMLibrary::new() {
        if let Ok(wmi) = WMIConnection::with_namespace_path("root\\WMI", com) {
            // Try MSAcpi_ThermalZoneTemperature
            let query =
                "SELECT InstanceName, CurrentTemperature FROM MSAcpi_ThermalZoneTemperature";
            let results: Result<Vec<HashMap<String, Variant>>, _> = wmi.raw_query(query);

            if let Ok(items) = results {
                for (idx, item) in items.iter().enumerate() {
                    if let Some(Variant::UI4(temp_tenths_kelvin)) = item.get("CurrentTemperature") {
                        // Convert from tenths of Kelvin to Celsius
                        let temp_c = (*temp_tenths_kelvin as f32 / 10.0) - 273.15;

                        // Get instance name if available
                        let name = item
                            .get("InstanceName")
                            .and_then(|v| match v {
                                Variant::String(s) => Some(s.clone()),
                                _ => None,
                            })
                            .unwrap_or_else(|| format!("Thermal Zone {}", idx));

                        if temp_c > 0.0 && temp_c < 150.0 {
                            // Try to classify the sensor based on name
                            let hw_type = if name.to_lowercase().contains("cpu") {
                                HwType::Cpu
                            } else if name.to_lowercase().contains("gpu") {
                                HwType::Gpu
                            } else {
                                HwType::Other
                            };

                            sensors.push(HwSensor {
                                name,
                                value: temp_c,
                                min: None,
                                max: None,
                                sensor_type: HwSensorType::Temperature,
                                hardware_type: hw_type,
                            });
                        }
                    }
                }
            }
        }
    }

    sensors
}

#[cfg(not(target_os = "windows"))]
pub fn read_wmi_temperatures() -> Vec<HwSensor> {
    Vec::new()
}
