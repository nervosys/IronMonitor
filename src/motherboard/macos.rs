// macOS motherboard and system monitoring implementation
//
// Data sources:
// - IOKit IOPlatformExpertDevice - System information
// - IOKit AppleSMC - SMC sensors (temperatures, fans, voltages)
// - system_profiler SPHardwareDataType - Hardware information
// - system_profiler SPSoftwareDataType - OS information
// - kextstat - Loaded kernel extensions (drivers)
// - ioreg - I/O registry for PCIe, USB, display, audio, bluetooth

use super::traits::*;
use std::process::Command;

/// macOS motherboard sensor (SMC-based)
pub struct MacSensor {
    name: String,
    temperatures: Vec<TemperatureSensor>,
    fans: Vec<FanInfo>,
}

impl MacSensor {
    pub fn new(name: String) -> Self {
        // Read thermal data via powermetrics (requires sudo) or ioreg
        let temperatures = Self::read_thermal_sensors();
        let fans = Self::read_fan_info();
        Self {
            name,
            temperatures,
            fans,
        }
    }

    fn read_thermal_sensors() -> Vec<TemperatureSensor> {
        let mut sensors = Vec::new();

        // Try ioreg for thermal sensors (AppleSMC)
        if let Ok(output) = Command::new("ioreg")
            .args(["-r", "-c", "AppleSmartBattery", "-d", "1"])
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            // Parse battery temperature if available
            if let Some(temp_line) = text.lines().find(|l| l.contains("\"Temperature\"")) {
                if let Some(val) = extract_ioreg_int(temp_line) {
                    sensors.push(TemperatureSensor {
                        label: "Battery".to_string(),
                        temperature: Some(val as f32 / 100.0),
                        max: Some(45.0),
                        critical: Some(55.0),
                        sensor_type: SensorType::Other,
                    });
                }
            }
        }

        // Try powermetrics for CPU die temperature (may require sudo)
        // Fallback: use IOKit thermal entries via ioreg
        if let Ok(output) = Command::new("ioreg")
            .args(["-r", "-c", "AppleARMIODevice", "-d", "1"])
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            if text.contains("pmgr") || text.contains("temp") {
                // On Apple Silicon, thermal data comes from pmp nodes
                // The sensor is reported because the machine has one; the
                // reading is not, because this path does not take one.
                //
                // This pushed `0.0` with a comment saying real values need an
                // IOKit C API it does not call — a die temperature of 0 °C,
                // beside a plausible 100 °C max and 110 °C critical, on the
                // strength of the word "temp" appearing in `ioreg` output.
                sensors.push(TemperatureSensor {
                    label: "SOC Die".to_string(),
                    temperature: None,
                    max: Some(100.0),
                    critical: Some(110.0),
                    sensor_type: SensorType::Cpu,
                });
            }
        }

        sensors
    }

    fn read_fan_info() -> Vec<FanInfo> {
        let mut fans = Vec::new();

        // ioreg -r -c AppleFanCtrl or AppleSMCFanControl
        if let Ok(output) = Command::new("ioreg")
            .args(["-r", "-n", "AppleSMCFamily", "-d", "3"])
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            // Look for fan entries
            let mut fan_idx = 0u32;
            for line in text.lines() {
                if line.contains("FanActualSpeed") || line.contains("Fan") {
                    if let Some(rpm) = extract_ioreg_int(line) {
                        fans.push(FanInfo {
                            label: format!("Fan {}", fan_idx),
                            rpm: if rpm > 0 { Some(rpm as u32) } else { None },
                            pwm: None,
                            min_rpm: None,
                            max_rpm: Some(6200),
                            controllable: false,
                        });
                        fan_idx += 1;
                    }
                }
            }
        }

        fans
    }
}

impl MotherboardDevice for MacSensor {
    fn name(&self) -> &str {
        &self.name
    }

    fn device_path(&self) -> Option<String> {
        Some("SMC".to_string())
    }

    fn temperature_sensors(&self) -> Result<Vec<TemperatureSensor>, Error> {
        Ok(self.temperatures.clone())
    }

    fn voltage_rails(&self) -> Result<Vec<VoltageRail>, Error> {
        // Voltage readings via SMC require IOKit C API bindings
        Ok(Vec::new())
    }

    fn fans(&self) -> Result<Vec<FanInfo>, Error> {
        Ok(self.fans.clone())
    }
}

/// Enumerate SMC sensors
pub fn enumerate() -> Result<Vec<Box<dyn MotherboardDevice>>, Error> {
    Ok(vec![Box::new(MacSensor::new("AppleSMC".to_string()))])
}

/// Run a command and return stdout
fn run_cmd(cmd: &str, args: &[&str]) -> Option<String> {
    Command::new(cmd)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
}

/// Extract integer value from ioreg line like `"Key" = 12345`
fn extract_ioreg_int(line: &str) -> Option<i64> {
    line.split('=')
        .nth(1)?
        .trim()
        .trim_end_matches(|c: char| !c.is_ascii_digit() && c != '-')
        .parse::<i64>()
        .ok()
}

/// Get system information via system_profiler and sysctl
pub fn get_system_info() -> Result<SystemInfo, Error> {
    // macOS version from sw_vers
    let os_version = run_cmd("sw_vers", &["-productVersion"])
        .unwrap_or_default()
        .trim()
        .to_string();
    let os_build = run_cmd("sw_vers", &["-buildVersion"])
        .unwrap_or_default()
        .trim()
        .to_string();
    let os_version_full = if os_build.is_empty() {
        os_version.clone()
    } else {
        format!("{} ({})", os_version, os_build)
    };

    // Kernel version
    let kernel_version = run_cmd("uname", &["-r"]).map(|s| s.trim().to_string());

    // Architecture
    let architecture = run_cmd("uname", &["-m"])
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| std::env::consts::ARCH.to_string());

    // Hostname
    let hostname = run_cmd("hostname", &[]).map(|s| s.trim().to_string());

    // Hardware info from system_profiler
    let hw_text = run_cmd("system_profiler", &["SPHardwareDataType"]).unwrap_or_default();

    let product_name = extract_profiler_field(&hw_text, "Model Name");
    let model_id = extract_profiler_field(&hw_text, "Model Identifier");
    let serial_number = extract_profiler_field(&hw_text, "Serial Number");
    let uuid = extract_profiler_field(&hw_text, "Hardware UUID");
    let cpu_name = extract_profiler_field(&hw_text, "Chip")
        .or_else(|| extract_profiler_field(&hw_text, "Processor Name"));
    let cpu_cores = extract_profiler_field(&hw_text, "Total Number of Cores").and_then(|s| {
        s.split_whitespace()
            .next()
            .and_then(|n| n.parse::<u32>().ok())
    });
    let cpu_threads =
        run_cmd("sysctl", &["-n", "hw.logicalcpu"]).and_then(|s| s.trim().parse::<u32>().ok());

    // Firmware version from ioreg
    let firmware_version = run_cmd("ioreg", &["-p", "IODeviceTree", "-n", "rom", "-d", "1"])
        .and_then(|text| {
            text.lines()
                .find(|l| l.contains("version"))
                .and_then(|l| l.split('\"').nth(3).map(|s| s.to_string()))
        });

    // Boot ROM version
    let boot_rom = extract_profiler_field(&hw_text, "Boot ROM Version").or(firmware_version);

    Ok(SystemInfo {
        os_name: "macOS".to_string(),
        os_version: os_version_full,
        kernel_version,
        architecture,
        hostname,
        bios: BiosInfo {
            vendor: Some("Apple".to_string()),
            version: boot_rom,
            release_date: None,
            revision: None,
            firmware_type: FirmwareType::Uefi,
            secure_boot: Some(true), // Apple Silicon always has Secure Boot
        },
        manufacturer: Some("Apple Inc.".to_string()),
        product_name,
        serial_number,
        uuid,
        board_vendor: Some("Apple Inc.".to_string()),
        board_name: model_id,
        board_version: None,
        cpu_name,
        cpu_cores,
        cpu_threads,
    })
}

/// Extract a field from system_profiler output
fn extract_profiler_field(text: &str, field: &str) -> Option<String> {
    text.lines()
        .find(|l| l.trim_start().starts_with(field))
        .and_then(|l| l.split(':').nth(1))
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Get driver/kext versions via kextstat and system_profiler
pub fn get_driver_versions() -> Result<Vec<DriverInfo>, Error> {
    let mut drivers = Vec::new();

    // Parse kextstat for loaded kernel extensions
    if let Some(text) = run_cmd("kextstat", &["-l"]) {
        for line in text.lines().skip(1) {
            // kextstat format: Index Refs Address Size Wired Name (Version) <Linked Against>
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 6 {
                let name = parts[5];
                let version = if parts.len() > 6 {
                    parts[6].trim_start_matches('(').trim_end_matches(')')
                } else {
                    "unknown"
                };

                // Classify the kext
                let driver_type = classify_kext(name);

                // Only include interesting kexts (skip apple internal plumbing)
                if should_include_kext(name) {
                    let vendor = if name.starts_with("com.apple.") {
                        Some("Apple".to_string())
                    } else {
                        name.split('.').nth(1).map(|s| s.to_string())
                    };

                    drivers.push(DriverInfo {
                        name: name.to_string(),
                        version: version.to_string(),
                        driver_type,
                        description: Some(kext_description(name)),
                        vendor,
                        date: None,
                    });
                }
            }
        }
    }

    // Also check system_profiler for driver extensions
    if let Some(text) = run_cmd("system_profiler", &["SPExtensionsDataType"]) {
        // Parse for key GPU/network/storage drivers
        let mut current_name = String::new();
        let mut current_version = String::new();
        for line in text.lines() {
            let trimmed = line.trim();
            if !trimmed.contains(':') && !trimmed.is_empty() && !trimmed.starts_with("Extensions:")
            {
                current_name = trimmed.trim_end_matches(':').to_string();
            } else if trimmed.starts_with("Version:") {
                current_version = trimmed.split(':').nth(1).unwrap_or("").trim().to_string();
                if !current_name.is_empty() && !current_version.is_empty() {
                    let dt = if current_name.to_lowercase().contains("gpu")
                        || current_name.to_lowercase().contains("graphics")
                    {
                        DriverType::Gpu
                    } else if current_name.to_lowercase().contains("network")
                        || current_name.to_lowercase().contains("wifi")
                    {
                        DriverType::Network
                    } else if current_name.to_lowercase().contains("storage")
                        || current_name.to_lowercase().contains("nvme")
                    {
                        DriverType::Storage
                    } else if current_name.to_lowercase().contains("audio") {
                        DriverType::Audio
                    } else {
                        DriverType::Other
                    };

                    // Avoid duplicates from kextstat
                    if !drivers.iter().any(|d: &DriverInfo| d.name == current_name) {
                        drivers.push(DriverInfo {
                            name: current_name.clone(),
                            version: current_version.clone(),
                            driver_type: dt,
                            description: None,
                            vendor: Some("Apple".to_string()),
                            date: None,
                        });
                    }
                }
            }
        }
    }

    Ok(drivers)
}

/// Get PCIe devices via system_profiler
pub fn get_pcie_devices() -> Result<Vec<PcieDeviceInfo>, Error> {
    let mut devices = Vec::new();

    // system_profiler SPPCIDataType for Thunderbolt/PCIe devices
    if let Some(text) = run_cmd("system_profiler", &["SPPCIDataType"]) {
        let mut current_name = String::new();
        let mut vendor = None;
        let mut device_id = None;
        let mut link_width: Option<u8> = None;
        let mut link_speed = None;
        let mut device_class = None;

        for line in text.lines() {
            let trimmed = line.trim();
            if !trimmed.contains(':') && !trimmed.is_empty() {
                // New device entry
                if !current_name.is_empty() {
                    devices.push(PcieDeviceInfo {
                        name: current_name.clone(),
                        device_id: device_id.take(),
                        vendor: vendor.take(),
                        pcie_version: None,
                        link_width: link_width.take(),
                        link_speed: link_speed.take(),
                        slot: None,
                        device_class: device_class.take(),
                    });
                }
                current_name = trimmed.trim_end_matches(':').to_string();
            } else if trimmed.starts_with("Vendor ID:") {
                vendor = trimmed.split(':').nth(1).map(|s| s.trim().to_string());
            } else if trimmed.starts_with("Device ID:") {
                device_id = trimmed.split(':').nth(1).map(|s| s.trim().to_string());
            } else if trimmed.starts_with("Link Width:") {
                link_width = trimmed
                    .split(':')
                    .nth(1)
                    .and_then(|s| s.trim().trim_start_matches('x').parse().ok());
            } else if trimmed.starts_with("Link Speed:") {
                link_speed = trimmed.split(':').nth(1).map(|s| s.trim().to_string());
            } else if trimmed.starts_with("Type:") {
                device_class = trimmed.split(':').nth(1).map(|s| s.trim().to_string());
            }
        }
        // Push last device
        if !current_name.is_empty() {
            devices.push(PcieDeviceInfo {
                name: current_name,
                device_id,
                vendor,
                pcie_version: None,
                link_width,
                link_speed,
                slot: None,
                device_class,
            });
        }
    }

    // Also Thunderbolt via SPThunderboltDataType
    if let Some(text) = run_cmd("system_profiler", &["SPThunderboltDataType"]) {
        let mut current_name = String::new();
        for line in text.lines() {
            let trimmed = line.trim();
            if !trimmed.contains(':') && !trimmed.is_empty() {
                current_name = trimmed.trim_end_matches(':').to_string();
            } else if trimmed.starts_with("Device Name:") {
                let dev_name = trimmed
                    .split(':')
                    .nth(1)
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();
                if !dev_name.is_empty() && !devices.iter().any(|d| d.name == dev_name) {
                    devices.push(PcieDeviceInfo {
                        name: dev_name,
                        device_id: None,
                        vendor: None,
                        pcie_version: None,
                        link_width: None,
                        link_speed: Some("Thunderbolt".to_string()),
                        slot: Some(current_name.clone()),
                        device_class: Some("Thunderbolt".to_string()),
                    });
                }
            }
        }
    }

    Ok(devices)
}

/// Get SATA/storage devices via system_profiler and diskutil
pub fn get_sata_devices() -> Result<Vec<SataDeviceInfo>, Error> {
    let mut devices = Vec::new();

    // system_profiler SPStorageDataType and SPNVMeDataType
    if let Some(text) = run_cmd("system_profiler", &["SPStorageDataType"]) {
        let mut current_name = String::new();
        let mut model = None;
        let mut serial = None;
        let mut capacity_str = None;
        let mut media_type = SataMediaType::Unknown;

        for line in text.lines() {
            let trimmed = line.trim();
            if !trimmed.contains(':') && !trimmed.is_empty() && !trimmed.starts_with("Storage:") {
                // New volume entry
                if !current_name.is_empty() {
                    let capacity_gb = parse_capacity_gb(capacity_str.as_deref());
                    devices.push(SataDeviceInfo {
                        name: current_name.clone(),
                        model: model.take(),
                        serial: serial.take(),
                        firmware: None,
                        capacity_gb,
                        interface_speed: Some("NVMe".to_string()),
                        port: None,
                        temperature: None,
                        media_type,
                    });
                    media_type = SataMediaType::Unknown;
                    capacity_str = None;
                }
                current_name = trimmed.trim_end_matches(':').to_string();
            } else if trimmed.starts_with("Physical Drive:") || trimmed.starts_with("Device Name:")
            {
                model = trimmed.split(':').nth(1).map(|s| s.trim().to_string());
            } else if trimmed.starts_with("Medium Type:") {
                let mt = trimmed
                    .split(':')
                    .nth(1)
                    .unwrap_or("")
                    .trim()
                    .to_lowercase();
                media_type = if mt.contains("ssd") || mt.contains("solid") {
                    SataMediaType::Ssd
                } else if mt.contains("hdd") || mt.contains("rotational") {
                    SataMediaType::Hdd
                } else {
                    SataMediaType::Ssd // Apple Silicon Macs are all SSD
                };
            } else if trimmed.starts_with("Capacity:") || trimmed.starts_with("Total Space:") {
                capacity_str = trimmed.split(':').nth(1).map(|s| s.trim().to_string());
            }
        }
        // Push last device
        if !current_name.is_empty() {
            let capacity_gb = parse_capacity_gb(capacity_str.as_deref());
            devices.push(SataDeviceInfo {
                name: current_name,
                model,
                serial,
                firmware: None,
                capacity_gb,
                interface_speed: Some("NVMe".to_string()),
                port: None,
                temperature: None,
                media_type: if media_type == SataMediaType::Unknown {
                    SataMediaType::Ssd
                } else {
                    media_type
                },
            });
        }
    }

    Ok(devices)
}

/// Parse capacity string like "500.07 GB (500,068,036,608 bytes)" to GB
fn parse_capacity_gb(s: Option<&str>) -> Option<f64> {
    let s = s?;
    // Try to extract bytes from parentheses
    if let Some(bytes_part) = s.split('(').nth(1) {
        let bytes_str: String = bytes_part.chars().filter(|c| c.is_ascii_digit()).collect();
        if let Ok(bytes) = bytes_str.parse::<u64>() {
            return Some(bytes as f64 / (1024.0 * 1024.0 * 1024.0));
        }
    }
    // Try to parse "500.07 GB" directly
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() >= 2 {
        if let Ok(val) = parts[0].parse::<f64>() {
            return match parts[1].to_uppercase().as_str() {
                "TB" => Some(val * 1024.0),
                "GB" => Some(val),
                "MB" => Some(val / 1024.0),
                _ => Some(val),
            };
        }
    }
    None
}

/// Get system temperatures
pub fn get_system_temperatures() -> Result<SystemTemperatures, Error> {
    // On macOS, temperatures primarily come from SMC via IOKit
    // Without IOKit C bindings, we use powermetrics or ioreg
    let storage_temps = Vec::new();

    Ok(SystemTemperatures {
        cpu: None, // Requires IOKit AppleSMC TC0P key
        gpu: None, // Requires IOKit AppleSMC TG0P key
        motherboard: None,
        storage: storage_temps,
        network: Vec::new(),
    })
}

/// Get peripheral devices
pub fn get_peripherals() -> Result<PeripheralsInfo, Error> {
    Ok(PeripheralsInfo {
        usb_devices: get_usb_devices(),
        display_outputs: get_display_outputs(),
        audio_devices: get_audio_devices(),
        bluetooth_devices: get_bluetooth_devices(),
        network_ports: get_network_ports(),
    })
}

fn get_usb_devices() -> Vec<UsbDeviceInfo> {
    let mut devices = Vec::new();
    if let Some(text) = run_cmd("system_profiler", &["SPUSBDataType"]) {
        let mut current_name = String::new();
        let mut vendor_id = None;
        let mut product_id = None;
        let mut vendor = None;
        let mut speed = None;

        for line in text.lines() {
            let trimmed = line.trim();
            // Device names are indented lines ending with ':'
            if trimmed.ends_with(':') && !trimmed.starts_with("USB:") && !trimmed.contains("Bus") {
                if !current_name.is_empty() {
                    devices.push(UsbDeviceInfo {
                        name: current_name.clone(),
                        device_id: product_id.clone(),
                        vendor: vendor.take(),
                        product_id: product_id.take(),
                        vendor_id: vendor_id.take(),
                        usb_version: classify_usb_speed(speed.as_deref()),
                        device_class: None,
                        status: Some("Connected".to_string()),
                        hub_port: None,
                    });
                    speed = None;
                }
                current_name = trimmed.trim_end_matches(':').to_string();
            } else if trimmed.starts_with("Vendor ID:") {
                vendor_id = trimmed
                    .split(':')
                    .nth(1)
                    .map(|s| s.split_whitespace().next().unwrap_or("").to_string());
                vendor = trimmed.split(':').nth(1).and_then(|s| {
                    let parts: Vec<&str> = s.trim().splitn(2, ' ').collect();
                    if parts.len() > 1 {
                        Some(
                            parts[1]
                                .trim_start_matches('(')
                                .trim_end_matches(')')
                                .to_string(),
                        )
                    } else {
                        None
                    }
                });
            } else if trimmed.starts_with("Product ID:") {
                product_id = trimmed
                    .split(':')
                    .nth(1)
                    .map(|s| s.split_whitespace().next().unwrap_or("").to_string());
            } else if trimmed.starts_with("Speed:") {
                speed = trimmed.split(':').nth(1).map(|s| s.trim().to_string());
            }
        }
        // Push last device
        if !current_name.is_empty() {
            devices.push(UsbDeviceInfo {
                name: current_name,
                device_id: product_id.clone(),
                vendor,
                product_id,
                vendor_id,
                usb_version: classify_usb_speed(speed.as_deref()),
                device_class: None,
                status: Some("Connected".to_string()),
                hub_port: None,
            });
        }
    }
    devices
}

fn classify_usb_speed(speed: Option<&str>) -> UsbVersion {
    match speed {
        Some(s) => {
            let lower = s.to_lowercase();
            if lower.contains("40 gb") || lower.contains("usb4") {
                UsbVersion::Usb4
            } else if lower.contains("20 gb") || lower.contains("3.2") {
                UsbVersion::Usb3_2
            } else if lower.contains("10 gb") || lower.contains("3.1") {
                UsbVersion::Usb3_1
            } else if lower.contains("5 gb") || lower.contains("super") {
                UsbVersion::Usb3_0
            } else if lower.contains("480") || lower.contains("high") {
                UsbVersion::Usb2_0
            } else if lower.contains("12") || lower.contains("full") {
                UsbVersion::Usb1_1
            } else {
                UsbVersion::Unknown
            }
        }
        None => UsbVersion::Unknown,
    }
}

fn get_display_outputs() -> Vec<DisplayOutputInfo> {
    let mut outputs = Vec::new();
    if let Some(text) = run_cmd("system_profiler", &["SPDisplaysDataType"]) {
        let mut current_display = String::new();
        let mut resolution = None;
        let mut connected = false;

        for line in text.lines() {
            let trimmed = line.trim();
            if !trimmed.contains(':')
                && !trimmed.is_empty()
                && !trimmed.starts_with("Graphics")
                && !trimmed.starts_with("Displays:")
            {
                if !current_display.is_empty() && connected {
                    let output_type = if current_display.to_lowercase().contains("built-in")
                        || current_display.to_lowercase().contains("internal")
                    {
                        DisplayOutputType::Internal
                    } else if current_display.to_lowercase().contains("thunderbolt") {
                        DisplayOutputType::Thunderbolt
                    } else if current_display.to_lowercase().contains("hdmi") {
                        DisplayOutputType::Hdmi
                    } else {
                        DisplayOutputType::DisplayPort
                    };
                    outputs.push(DisplayOutputInfo {
                        name: current_display.clone(),
                        output_type,
                        connected: true,
                        resolution: resolution.take(),
                        refresh_rate: None,
                        adapter: None,
                    });
                }
                current_display = trimmed.trim_end_matches(':').to_string();
                connected = false;
            } else if trimmed.starts_with("Resolution:") {
                resolution = trimmed.split(':').nth(1).map(|s| s.trim().to_string());
                connected = true;
            } else if trimmed.starts_with("Main Display:") && trimmed.contains("Yes") {
                connected = true;
            }
        }
        // Push last display
        if !current_display.is_empty() && connected {
            let output_type = if current_display.to_lowercase().contains("built-in")
                || current_display.to_lowercase().contains("internal")
            {
                DisplayOutputType::Internal
            } else {
                DisplayOutputType::DisplayPort
            };
            outputs.push(DisplayOutputInfo {
                name: current_display,
                output_type,
                connected: true,
                resolution,
                refresh_rate: None,
                adapter: None,
            });
        }
    }
    outputs
}

fn get_audio_devices() -> Vec<AudioDeviceInfo> {
    let mut devices = Vec::new();
    if let Some(text) = run_cmd("system_profiler", &["SPAudioDataType"]) {
        let mut current_name = String::new();
        let mut device_type = AudioDeviceType::Unknown;
        let mut manufacturer = None;
        let mut is_default = false;

        for line in text.lines() {
            let trimmed = line.trim();
            if !trimmed.contains(':') && !trimmed.is_empty() && !trimmed.starts_with("Audio:") {
                if !current_name.is_empty() {
                    devices.push(AudioDeviceInfo {
                        name: current_name.clone(),
                        device_type,
                        manufacturer: manufacturer.take(),
                        status: Some("Active".to_string()),
                        is_default,
                    });
                    device_type = AudioDeviceType::Unknown;
                    is_default = false;
                }
                current_name = trimmed.trim_end_matches(':').to_string();
            } else if trimmed.starts_with("Output Channels:") || trimmed.starts_with("Output:") {
                device_type = if device_type == AudioDeviceType::Input {
                    AudioDeviceType::OutputInput
                } else {
                    AudioDeviceType::Output
                };
            } else if trimmed.starts_with("Input Channels:") || trimmed.starts_with("Input:") {
                device_type = if device_type == AudioDeviceType::Output {
                    AudioDeviceType::OutputInput
                } else {
                    AudioDeviceType::Input
                };
            } else if trimmed.starts_with("Manufacturer:") {
                manufacturer = trimmed.split(':').nth(1).map(|s| s.trim().to_string());
            } else if trimmed.starts_with("Default Output Device:") && trimmed.contains("Yes") {
                is_default = true;
            }
        }
        // Push last device
        if !current_name.is_empty() {
            devices.push(AudioDeviceInfo {
                name: current_name,
                device_type,
                manufacturer,
                status: Some("Active".to_string()),
                is_default,
            });
        }
    }
    devices
}

fn get_bluetooth_devices() -> Vec<BluetoothDeviceInfo> {
    let mut devices = Vec::new();
    if let Some(text) = run_cmd("system_profiler", &["SPBluetoothDataType"]) {
        let mut current_name = String::new();
        let mut address = None;
        let mut connected = false;
        let mut device_type = None;

        for line in text.lines() {
            let trimmed = line.trim();
            if !trimmed.contains(':')
                && !trimmed.is_empty()
                && !trimmed.starts_with("Bluetooth:")
                && !trimmed.starts_with("Connected:")
                && !trimmed.starts_with("Not Connected:")
            {
                if !current_name.is_empty() {
                    devices.push(BluetoothDeviceInfo {
                        name: current_name.clone(),
                        address: address.take(),
                        device_type: device_type.take(),
                        connected,
                        paired: true,
                    });
                    connected = false;
                }
                current_name = trimmed.trim_end_matches(':').to_string();
            } else if trimmed.starts_with("Address:") {
                address = trimmed.split(':').nth(1).map(|s| s.trim().to_string());
            } else if trimmed.starts_with("Minor Type:") || trimmed.starts_with("Type:") {
                device_type = trimmed.split(':').nth(1).map(|s| s.trim().to_string());
            } else if trimmed == "Connected:" {
                connected = true;
            } else if trimmed == "Not Connected:" {
                connected = false;
            }
        }
        // Push last device
        if !current_name.is_empty() {
            devices.push(BluetoothDeviceInfo {
                name: current_name,
                address,
                device_type,
                connected,
                paired: true,
            });
        }
    }
    devices
}

fn get_network_ports() -> Vec<NetworkPortInfo> {
    let mut ports = Vec::new();
    if let Some(text) = run_cmd("networksetup", &["-listnetworkserviceorder"]) {
        for line in text.lines() {
            let trimmed = line.trim();
            // Lines like: (1) Wi-Fi
            if trimmed.starts_with('(') {
                if let Some(name_part) = trimmed.split(')').nth(1) {
                    let name = name_part.trim().to_string();
                    if name.is_empty() {
                        continue;
                    }
                    let port_type = if name.to_lowercase().contains("wi-fi")
                        || name.to_lowercase().contains("airport")
                    {
                        NetworkPortType::WiFi
                    } else if name.to_lowercase().contains("bluetooth") {
                        NetworkPortType::Bluetooth
                    } else if name.to_lowercase().contains("thunderbolt") {
                        NetworkPortType::Thunderbolt
                    } else if name.to_lowercase().contains("ethernet")
                        || name.to_lowercase().contains("lan")
                    {
                        NetworkPortType::Ethernet
                    } else {
                        NetworkPortType::Other
                    };

                    // Check if active via ifconfig
                    let connected = run_cmd("ifconfig", &[])
                        .map(|t| t.contains("status: active"))
                        .unwrap_or(false);

                    ports.push(NetworkPortInfo {
                        name,
                        port_type,
                        speed: None,
                        mac_address: None,
                        connected,
                    });
                }
            }
        }
    }
    ports
}

/// Classify a kext by name
fn classify_kext(name: &str) -> DriverType {
    let lower = name.to_lowercase();
    if lower.contains("graphic")
        || lower.contains("gpu")
        || lower.contains("agx")
        || lower.contains("metal")
    {
        DriverType::Gpu
    } else if lower.contains("nvme")
        || lower.contains("storage")
        || lower.contains("ahci")
        || lower.contains("disk")
        || lower.contains("apfs")
    {
        DriverType::Storage
    } else if lower.contains("network")
        || lower.contains("wifi")
        || lower.contains("ethernet")
        || lower.contains("80211")
        || lower.contains("bcm")
    {
        DriverType::Network
    } else if lower.contains("audio") || lower.contains("sound") || lower.contains("coreaudio") {
        DriverType::Audio
    } else if lower.contains("usb") || lower.contains("xhci") {
        DriverType::Usb
    } else if lower.contains("chipset") || lower.contains("platform") || lower.contains("pci") {
        DriverType::Chipset
    } else {
        DriverType::Other
    }
}

/// Filter out uninteresting internal kexts
fn should_include_kext(name: &str) -> bool {
    let lower = name.to_lowercase();
    // Include GPU, storage, network, audio, USB, and platform kexts
    lower.contains("graphic") || lower.contains("gpu") || lower.contains("agx") || lower.contains("metal")
        || lower.contains("nvme") || lower.contains("storage") || lower.contains("ahci") || lower.contains("apfs")
        || lower.contains("network") || lower.contains("wifi") || lower.contains("ethernet") || lower.contains("80211")
        || lower.contains("audio") || lower.contains("sound")
        || lower.contains("usb") || lower.contains("xhci")
        || lower.contains("thunderbolt")
        || lower.contains("bluetooth")
        // Or if it's a third-party kext
        || !lower.starts_with("com.apple.")
}

/// Get a brief description for a known kext
fn kext_description(name: &str) -> String {
    let lower = name.to_lowercase();
    if lower.contains("agxg") {
        "Apple GPU Accelerator".to_string()
    } else if lower.contains("agxmetal") {
        "Apple Metal GPU Framework".to_string()
    } else if lower.contains("iographics") {
        "Graphics Display Framework".to_string()
    } else if lower.contains("applebcm") || lower.contains("80211") {
        "Apple WiFi/Bluetooth".to_string()
    } else if lower.contains("nvme") {
        "NVMe Storage Controller".to_string()
    } else if lower.contains("apfs") {
        "Apple File System".to_string()
    } else if lower.contains("usb") || lower.contains("xhci") {
        "USB Host Controller".to_string()
    } else if lower.contains("thunderbolt") {
        "Thunderbolt Controller".to_string()
    } else if lower.contains("audio") || lower.contains("sound") {
        "Audio Driver".to_string()
    } else if lower.contains("ethernet") {
        "Ethernet Controller".to_string()
    } else {
        format!("Kernel Extension: {}", name)
    }
}
