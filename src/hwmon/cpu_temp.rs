// CPU Temperature reading via MSR (Model-Specific Registers)
//
// Intel CPUs: Read IA32_THERM_STATUS (MSR 0x19C) and IA32_TEMPERATURE_TARGET (MSR 0x1A2)
// AMD CPUs: Read THM_TCON_CUR_TMP via SMN or use Tctl/Tdie from MSR

use super::{HwSensor, HwSensorType, HwType};

/// Intel MSR addresses
#[allow(dead_code)]
const IA32_THERM_STATUS: u32 = 0x19C;
#[allow(dead_code)]
const IA32_TEMPERATURE_TARGET: u32 = 0x1A2;
#[allow(dead_code)]
const IA32_PACKAGE_THERM_STATUS: u32 = 0x1B1;

/// AMD MSR addresses  
#[allow(dead_code)]
const AMD_MSR_HARDWARE_THERMAL_CONTROL: u32 = 0xC001_0059;
#[allow(dead_code)]
const AMD_MSR_REPORTED_TEMP_CONTROL: u32 = 0xC001_0063;

/// Read CPU temperatures from all available sources
pub fn read_cpu_temperatures() -> Vec<HwSensor> {
    let mut sensors = Vec::new();

    #[cfg(target_os = "windows")]
    {
        // Try Intel DTS first
        if let Some(temps) = read_intel_dts() {
            sensors.extend(temps);
        }

        // Try AMD Tctl/Tdie
        if sensors.is_empty() {
            if let Some(temps) = read_amd_temps() {
                sensors.extend(temps);
            }
        }

        // Fallback: Try Performance Counters
        if sensors.is_empty() {
            if let Some(temps) = read_from_performance_counters() {
                sensors.extend(temps);
            }
        }

        // Fallback: Try CPU-Z style registry
        if sensors.is_empty() {
            if let Some(temps) = read_from_registry() {
                sensors.extend(temps);
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        sensors.extend(read_linux_cpu_temps());
    }

    sensors
}

#[cfg(target_os = "windows")]
fn read_intel_dts() -> Option<Vec<HwSensor>> {
    // Reading MSRs requires either:
    // 1. A kernel driver (like WinRing0)
    // 2. The DeviceIoControl API with a compatible driver
    //
    // Without a kernel driver, we cannot directly read MSRs on Windows.
    // We'll try alternative methods below.

    None
}

#[cfg(target_os = "windows")]
fn read_amd_temps() -> Option<Vec<HwSensor>> {
    // AMD CPUs expose temperature via SMN (System Management Network)
    // accessed through PCI config space at D0:F0 (root complex)
    //
    // Registers:
    // - SMN address: 0x60 (SMU_ARGS)
    // - SMN data: 0x64 (SMU_RESULT)
    // - THM_TCON_CUR_TMP offset: 0x00059800
    //
    // This also requires kernel-level access

    None
}

#[cfg(target_os = "windows")]
fn read_from_performance_counters() -> Option<Vec<HwSensor>> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::System::Performance::{
        PdhAddEnglishCounterW, PdhCollectQueryData, PdhGetFormattedCounterValue, PdhOpenQueryW,
        PDH_FMT_COUNTERVALUE, PDH_FMT_DOUBLE,
    };

    // Try reading thermal zone info via Performance Counters
    // Path: \Thermal Zone Information(*)\Temperature

    unsafe {
        let mut query = std::mem::zeroed();
        let result = PdhOpenQueryW(PCWSTR::null(), 0, &mut query);
        if result != 0 {
            return None;
        }

        // Counter path for thermal zones
        let counter_path: Vec<u16> = OsStr::new("\\Thermal Zone Information(*)\\Temperature")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let mut counter = std::mem::zeroed();
        let result = PdhAddEnglishCounterW(
            query,
            PCWSTR::from_raw(counter_path.as_ptr()),
            0,
            &mut counter,
        );

        if result != 0 {
            return None;
        }

        // Collect data
        let result = PdhCollectQueryData(query);
        if result != 0 {
            return None;
        }

        // Get value
        let mut value: PDH_FMT_COUNTERVALUE = std::mem::zeroed();
        let result = PdhGetFormattedCounterValue(counter, PDH_FMT_DOUBLE, None, &mut value);

        if result == 0 {
            // Convert from Kelvin (PDH reports in tenths of Kelvin) to Celsius
            let kelvin = value.Anonymous.doubleValue / 10.0;
            let celsius = kelvin - 273.15;

            if celsius > 0.0 && celsius < 150.0 {
                return Some(vec![HwSensor {
                    name: "CPU Package".to_string(),
                    value: celsius as f32,
                    min: None,
                    max: None,
                    sensor_type: HwSensorType::Temperature,
                    hardware_type: HwType::Cpu,
                }]);
            }
        }
    }

    None
}

#[cfg(target_os = "windows")]
fn read_from_registry() -> Option<Vec<HwSensor>> {
    use winreg::enums::*;
    use winreg::RegKey;

    // Some motherboard utilities write temperatures to registry
    // Try common locations

    let locations = [
        (HKEY_LOCAL_MACHINE, r"HARDWARE\ACPI\THERMAL_ZONE"),
        (
            HKEY_LOCAL_MACHINE,
            r"SYSTEM\CurrentControlSet\Control\Class\{4d36e97d-e325-11ce-bfc1-08002be10318}",
        ),
    ];

    for (hive, path) in &locations {
        if let Ok(key) = RegKey::predef(*hive).open_subkey(path) {
            // Try to read temperature values
            for name in key.enum_keys().filter_map(|k| k.ok()) {
                if let Ok(subkey) = key.open_subkey(&name) {
                    // Check for temperature-related values
                    if let Ok(temp) = subkey.get_value::<u32, _>("CurrentTemperature") {
                        // ACPI thermal zone temps are in tenths of Kelvin
                        let kelvin = temp as f64 / 10.0;
                        let celsius = kelvin - 273.15;

                        if celsius > 0.0 && celsius < 150.0 {
                            return Some(vec![HwSensor {
                                name: format!("Thermal Zone {}", name),
                                value: celsius as f32,
                                min: None,
                                max: None,
                                sensor_type: HwSensorType::Temperature,
                                hardware_type: HwType::Cpu,
                            }]);
                        }
                    }
                }
            }
        }
    }

    None
}

#[cfg(target_os = "linux")]
fn read_linux_cpu_temps() -> Vec<HwSensor> {
    use std::fs;
    use std::path::Path;

    let mut sensors = Vec::new();
    let hwmon_path = Path::new("/sys/class/hwmon");

    if let Ok(entries) = fs::read_dir(hwmon_path) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();

            // Check if this is a CPU temperature sensor (coretemp, k10temp, etc.)
            let name_path = path.join("name");
            let name = fs::read_to_string(&name_path)
                .map(|s| s.trim().to_string())
                .unwrap_or_default();

            if name.contains("coretemp") || name.contains("k10temp") || name.contains("cpu") {
                // Read all temp*_input files
                for i in 1..=16 {
                    let input_path = path.join(format!("temp{}_input", i));
                    let label_path = path.join(format!("temp{}_label", i));

                    if let Ok(temp_str) = fs::read_to_string(&input_path) {
                        if let Ok(temp_millicelsius) = temp_str.trim().parse::<i32>() {
                            let temp_celsius = temp_millicelsius as f32 / 1000.0;

                            // Without a label, `tempN` is not known to be core N-1:
                            // temp1 is the package or Tctl on most chips, and the
                            // label file is what says which. The old fallback
                            // named it "Core {N-1}" regardless. Name it by the
                            // input it came from instead.
                            let label = fs::read_to_string(&label_path)
                                .map(|s| s.trim().to_string())
                                .unwrap_or_else(|_| format!("{} temp{}", name, i));

                            sensors.push(HwSensor {
                                name: label,
                                value: temp_celsius,
                                min: None,
                                max: None,
                                sensor_type: HwSensorType::Temperature,
                                hardware_type: HwType::Cpu,
                            });
                        }
                    }
                }
            }
        }
    }

    sensors
}
