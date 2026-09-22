// Linux motherboard and system monitoring implementation
//
// Data sources:
// - /sys/class/hwmon/* - Hardware monitoring sensors
// - /sys/class/dmi/id/* - DMI/SMBIOS system information
// - /sys/firmware/efi - EFI/UEFI detection
// - /proc/cpuinfo - CPU information
// - /sys/module/*/version - Kernel module versions
// - lsmod - Loaded kernel modules

use super::traits::*;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Linux motherboard sensor device
pub struct LinuxSensor {
    hwmon_path: PathBuf,
    /// Contents of the hwmon `name` file — the chip, e.g. `coretemp`.
    ///
    /// There used to be a `name` field alongside this holding the directory name
    /// (`hwmon0`). Nothing ever read it: `name()` returns the chip, which is what a
    /// reader can act on. Clippy flagged the getter as returning the wrong field;
    /// the getter was right and the field was dead.
    chip_name: String,
}

impl LinuxSensor {
    /// Create a new Linux sensor from a hwmon path
    pub fn new(hwmon_path: PathBuf) -> Result<Self, Error> {
        let name_path = hwmon_path.join("name");
        let chip_name = fs::read_to_string(&name_path)
            .map_err(|e| Error::InitializationFailed(format!("Failed to read sensor name: {}", e)))?
            .trim()
            .to_string();

        Ok(Self {
            hwmon_path,
            chip_name,
        })
    }

    /// Read a sensor input file
    fn read_input(&self, pattern: &str, index: u32) -> Option<i64> {
        let path = self.hwmon_path.join(format!("{}{}_input", pattern, index));
        fs::read_to_string(path).ok()?.trim().parse::<i64>().ok()
    }

    /// Read a sensor label file
    fn read_label(&self, pattern: &str, index: u32) -> Option<String> {
        let path = self.hwmon_path.join(format!("{}{}_label", pattern, index));
        fs::read_to_string(path).ok().map(|s| s.trim().to_string())
    }

    /// Read a sensor max file
    fn read_max(&self, pattern: &str, index: u32) -> Option<i64> {
        let path = self.hwmon_path.join(format!("{}{}_max", pattern, index));
        fs::read_to_string(path).ok()?.trim().parse::<i64>().ok()
    }

    /// Read a sensor critical file
    fn read_crit(&self, pattern: &str, index: u32) -> Option<i64> {
        let path = self.hwmon_path.join(format!("{}{}_crit", pattern, index));
        fs::read_to_string(path).ok()?.trim().parse::<i64>().ok()
    }

    /// Determine sensor type from label
    fn classify_sensor(label: &str) -> SensorType {
        let label_lower = label.to_lowercase();
        if label_lower.contains("cpu")
            || label_lower.contains("core")
            || label_lower.contains("package")
        {
            SensorType::Cpu
        } else if label_lower.contains("chipset") || label_lower.contains("pch") {
            SensorType::Chipset
        } else if label_lower.contains("vrm") || label_lower.contains("vcore") {
            SensorType::Vrm
        } else if label_lower.contains("ambient") || label_lower.contains("system") {
            SensorType::Ambient
        } else if label_lower.contains("m.2") || label_lower.contains("nvme") {
            SensorType::M2Slot
        } else {
            SensorType::Other
        }
    }
}

impl MotherboardDevice for LinuxSensor {
    // The chip name, not the hwmon directory: `coretemp` tells a reader what they
    // are looking at; `hwmon0` tells them where it happened to be enumerated.
    #[allow(clippy::misnamed_getters)]
    fn name(&self) -> &str {
        &self.chip_name
    }

    fn device_path(&self) -> Option<String> {
        Some(self.hwmon_path.to_string_lossy().to_string())
    }

    fn temperature_sensors(&self) -> Result<Vec<TemperatureSensor>, Error> {
        let mut sensors = Vec::new();

        // Try temperature inputs (temp1_input through temp32_input)
        for i in 1..=32 {
            if let Some(temp_millic) = self.read_input("temp", i) {
                let label = self
                    .read_label("temp", i)
                    .unwrap_or_else(|| format!("temp{}", i));

                let temperature = temp_millic as f32 / 1000.0;
                let max = self.read_max("temp", i).map(|v| v as f32 / 1000.0);
                let critical = self.read_crit("temp", i).map(|v| v as f32 / 1000.0);
                let sensor_type = Self::classify_sensor(&label);

                sensors.push(TemperatureSensor {
                    label,
                    // A real hwmon reading; the read above is already guarded.
                    temperature: Some(temperature),
                    max,
                    critical,
                    sensor_type,
                });
            }
        }

        Ok(sensors)
    }

    fn voltage_rails(&self) -> Result<Vec<VoltageRail>, Error> {
        let mut rails = Vec::new();

        // Try voltage inputs (in0_input through in32_input)
        for i in 0..=32 {
            if let Some(voltage_milliv) = self.read_input("in", i) {
                let label = self
                    .read_label("in", i)
                    .unwrap_or_else(|| format!("in{}", i));

                let voltage = voltage_milliv as f32 / 1000.0;
                let min = self.read_max("in", i).map(|v| v as f32 / 1000.0);
                let max = self.read_crit("in", i).map(|v| v as f32 / 1000.0);

                rails.push(VoltageRail {
                    label,
                    voltage: Some(voltage),
                    min,
                    max,
                });
            }
        }

        Ok(rails)
    }

    fn fans(&self) -> Result<Vec<FanInfo>, Error> {
        let mut fans = Vec::new();

        // Try fan inputs (fan1_input through fan16_input)
        for i in 1..=16 {
            if let Some(rpm) = self.read_input("fan", i) {
                let label = self
                    .read_label("fan", i)
                    .unwrap_or_else(|| format!("fan{}", i));

                // Try to read PWM value
                let pwm_path = self.hwmon_path.join(format!("pwm{}", i));
                let pwm = fs::read_to_string(&pwm_path)
                    .ok()
                    .and_then(|s| s.trim().parse::<u8>().ok());

                // Check if PWM is writable (controllable)
                let pwm_enable_path = self.hwmon_path.join(format!("pwm{}_enable", i));
                let controllable = pwm_enable_path.exists()
                    && fs::metadata(&pwm_enable_path)
                        .map(|m| !m.permissions().readonly())
                        .unwrap_or(false);

                let rpm_value = if rpm > 0 { Some(rpm as u32) } else { None };

                fans.push(FanInfo {
                    label,
                    rpm: rpm_value,
                    pwm,
                    min_rpm: None,
                    max_rpm: None,
                    controllable,
                });
            }
        }

        Ok(fans)
    }

    fn set_fan_speed(&self, fan_index: usize, speed: FanControl) -> Result<(), Error> {
        let pwm_path = self.hwmon_path.join(format!("pwm{}", fan_index + 1));
        let pwm_enable_path = self.hwmon_path.join(format!("pwm{}_enable", fan_index + 1));

        if !pwm_path.exists() {
            return Err(Error::FanControlError(format!(
                "Fan {} does not support PWM control",
                fan_index
            )));
        }

        match speed {
            FanControl::Manual(pwm_value) => {
                // Set to manual mode (pwm_enable = 1)
                fs::write(&pwm_enable_path, "1\n").map_err(|e| {
                    Error::PermissionDenied(format!("Failed to set fan mode: {}", e))
                })?;

                // Set PWM value
                fs::write(&pwm_path, format!("{}\n", pwm_value)).map_err(|e| {
                    Error::PermissionDenied(format!("Failed to set fan speed: {}", e))
                })?;
            }
            FanControl::Automatic => {
                // Set to automatic mode (pwm_enable = 2 or 5 depending on chip)
                fs::write(&pwm_enable_path, "2\n").map_err(|e| {
                    Error::PermissionDenied(format!("Failed to set fan mode: {}", e))
                })?;
            }
        }

        Ok(())
    }
}

/// Enumerate all hwmon sensors
pub fn enumerate() -> Result<Vec<Box<dyn MotherboardDevice>>, Error> {
    let hwmon_dir = Path::new("/sys/class/hwmon");

    if !hwmon_dir.exists() {
        return Err(Error::NoSensorsFound);
    }

    let mut devices: Vec<Box<dyn MotherboardDevice>> = Vec::new();

    for entry in fs::read_dir(hwmon_dir).map_err(Error::IoError)? {
        let entry = entry.map_err(Error::IoError)?;
        let path = entry.path();

        // Skip if not a directory
        if !path.is_dir() {
            continue;
        }

        // Try to create a sensor device
        if let Ok(sensor) = LinuxSensor::new(path) {
            devices.push(Box::new(sensor));
        }
    }

    if devices.is_empty() {
        Err(Error::NoSensorsFound)
    } else {
        Ok(devices)
    }
}

/// Read DMI/SMBIOS information
fn read_dmi(path: &str) -> Option<String> {
    fs::read_to_string(Path::new("/sys/class/dmi/id").join(path))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Detect firmware type (BIOS or UEFI)
fn detect_firmware_type() -> FirmwareType {
    if Path::new("/sys/firmware/efi").exists() {
        FirmwareType::Uefi
    } else {
        FirmwareType::Bios
    }
}

/// Get system information
pub fn get_system_info() -> Result<SystemInfo, Error> {
    // OS information
    let os_release = fs::read_to_string("/etc/os-release")
        .or_else(|_| fs::read_to_string("/usr/lib/os-release"))
        .unwrap_or_default();

    let mut os_info = HashMap::new();
    for line in os_release.lines() {
        if let Some((key, value)) = line.split_once('=') {
            os_info.insert(key.to_string(), value.trim_matches('"').to_string());
        }
    }

    let os_name = os_info
        .get("PRETTY_NAME")
        .or_else(|| os_info.get("NAME"))
        .cloned()
        .unwrap_or_else(|| "Linux".to_string());

    let os_version = os_info
        .get("VERSION")
        .or_else(|| os_info.get("VERSION_ID"))
        .cloned()
        .unwrap_or_else(|| "Unknown".to_string());

    // Kernel version
    let kernel_version = fs::read_to_string("/proc/version")
        .ok()
        .and_then(|v| v.split_whitespace().nth(2).map(String::from));

    // Architecture
    let architecture = std::env::consts::ARCH.to_string();

    // Hostname
    let hostname = fs::read_to_string("/etc/hostname")
        .ok()
        .map(|s| s.trim().to_string());

    // BIOS information
    let bios = BiosInfo {
        vendor: read_dmi("bios_vendor"),
        version: read_dmi("bios_version"),
        release_date: read_dmi("bios_date"),
        revision: None,
        firmware_type: detect_firmware_type(),
        secure_boot: None, // Would need to parse /sys/firmware/efi/efivars/SecureBoot-*
    };

    // Hardware information
    let manufacturer = read_dmi("sys_vendor");
    let product_name = read_dmi("product_name");
    let serial_number = read_dmi("product_serial");
    let uuid = read_dmi("product_uuid");

    let board_vendor = read_dmi("board_vendor");
    let board_name = read_dmi("board_name");
    let board_version = read_dmi("board_version");

    // CPU information
    let cpuinfo = fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let cpu_name = cpuinfo
        .lines()
        .find(|line| line.starts_with("model name"))
        .and_then(|line| line.split(':').nth(1))
        .map(|s| s.trim().to_string());

    let cpu_cores = cpuinfo
        .lines()
        .find(|line| line.starts_with("cpu cores"))
        .and_then(|line| line.split(':').nth(1))
        .and_then(|s| s.trim().parse::<u32>().ok());

    let cpu_threads = cpuinfo
        .lines()
        .filter(|line| line.starts_with("processor"))
        .count() as u32;

    let cpu_threads = if cpu_threads > 0 {
        Some(cpu_threads)
    } else {
        None
    };

    Ok(SystemInfo {
        os_name,
        os_version,
        kernel_version,
        architecture,
        hostname,
        bios,
        manufacturer,
        product_name,
        serial_number,
        uuid,
        board_vendor,
        board_name,
        board_version,
        cpu_name,
        cpu_cores,
        cpu_threads,
    })
}

/// Get driver/module versions
pub fn get_driver_versions() -> Result<Vec<DriverInfo>, Error> {
    let mut drivers = Vec::new();

    // GPU drivers
    if let Ok(version) = fs::read_to_string("/sys/module/nvidia/version") {
        drivers.push(DriverInfo {
            name: "nvidia".to_string(),
            version: version.trim().to_string(),
            driver_type: DriverType::Gpu,
            description: Some("NVIDIA GPU Driver".to_string()),
            vendor: Some("NVIDIA".to_string()),
            date: None,
        });
    }

    if let Ok(version) = fs::read_to_string("/sys/module/amdgpu/version") {
        drivers.push(DriverInfo {
            name: "amdgpu".to_string(),
            version: version.trim().to_string(),
            driver_type: DriverType::Gpu,
            description: Some("AMD GPU Driver".to_string()),
            vendor: Some("AMD".to_string()),
            date: None,
        });
    }

    if let Ok(version) = fs::read_to_string("/sys/module/i915/version") {
        drivers.push(DriverInfo {
            name: "i915".to_string(),
            version: version.trim().to_string(),
            driver_type: DriverType::Gpu,
            description: Some("Intel GPU Driver".to_string()),
            vendor: Some("Intel".to_string()),
            date: None,
        });
    }

    // Storage drivers
    for module in &["nvme", "ahci", "sata_nv", "megaraid_sas"] {
        let version_path = format!("/sys/module/{}/version", module);
        if let Ok(version) = fs::read_to_string(&version_path) {
            drivers.push(DriverInfo {
                name: module.to_string(),
                version: version.trim().to_string(),
                driver_type: DriverType::Storage,
                description: Some(format!("{} Storage Driver", module.to_uppercase())),
                vendor: None,
                date: None,
            });
        }
    }

    // Network drivers (common ones)
    for module in &["e1000e", "igb", "ixgbe", "r8169", "bnx2x"] {
        let version_path = format!("/sys/module/{}/version", module);
        if let Ok(version) = fs::read_to_string(&version_path) {
            drivers.push(DriverInfo {
                name: module.to_string(),
                version: version.trim().to_string(),
                driver_type: DriverType::Network,
                description: Some(format!("{} Network Driver", module.to_uppercase())),
                vendor: None,
                date: None,
            });
        }
    }

    Ok(drivers)
}

/// Get PCIe devices from /sys/bus/pci/devices
pub fn get_pcie_devices() -> Result<Vec<PcieDeviceInfo>, Error> {
    let pci_dir = Path::new("/sys/bus/pci/devices");
    if !pci_dir.exists() {
        return Ok(Vec::new());
    }

    let mut devices = Vec::new();

    for entry in fs::read_dir(pci_dir).map_err(Error::IoError)? {
        let entry = entry.map_err(Error::IoError)?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let bus_id = entry.file_name().to_string_lossy().to_string();

        // Read PCI class
        let class = fs::read_to_string(path.join("class"))
            .ok()
            .map(|s| s.trim().to_string());

        // Read vendor and device IDs
        let vendor_id = fs::read_to_string(path.join("vendor"))
            .ok()
            .map(|s| s.trim().to_string());
        let device_id = fs::read_to_string(path.join("device"))
            .ok()
            .map(|s| s.trim().to_string());

        // Classify device from PCI class code
        let device_class = class.as_deref().map(classify_pci_class);

        // Read link speed/width from /sys/bus/pci/devices/XXXX:XX:XX.X/
        let current_link_speed = fs::read_to_string(path.join("current_link_speed"))
            .ok()
            .map(|s| s.trim().to_string());
        let current_link_width = fs::read_to_string(path.join("current_link_width"))
            .ok()
            .and_then(|s| s.trim().parse::<u8>().ok());

        // Derive PCIe version from link speed string
        let pcie_version = current_link_speed.as_deref().map(|s| {
            if s.contains("16") || s.contains("32") {
                "PCIe 5.0".to_string()
            } else if s.contains("16") {
                "PCIe 4.0".to_string()
            } else if s.contains("8") {
                "PCIe 3.0".to_string()
            } else if s.contains("5") {
                "PCIe 2.0".to_string()
            } else if s.contains("2.5") {
                "PCIe 1.0".to_string()
            } else {
                s.to_string()
            }
        });

        // Try to get device name from uevent or modalias
        let name = fs::read_to_string(path.join("label"))
            .ok()
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| {
                // Fallback: construct from bus_id and class
                format!(
                    "PCI {} ({})",
                    bus_id,
                    device_class.as_deref().unwrap_or("Unknown")
                )
            });

        devices.push(PcieDeviceInfo {
            name,
            device_id,
            vendor: vendor_id,
            pcie_version,
            link_width: current_link_width,
            link_speed: current_link_speed,
            slot: Some(bus_id),
            device_class,
        });
    }

    Ok(devices)
}

/// Classify a PCI class code into a human-readable category
fn classify_pci_class(class_hex: &str) -> String {
    // PCI class codes: 0xCCSSPP (class, subclass, prog-if)
    let class_code = u32::from_str_radix(class_hex.trim_start_matches("0x"), 16).unwrap_or(0);
    let class_id = (class_code >> 16) & 0xFF;
    match class_id {
        0x00 => "Unclassified".to_string(),
        0x01 => "Storage".to_string(),
        0x02 => "Network".to_string(),
        0x03 => "Display/VGA".to_string(),
        0x04 => "Multimedia".to_string(),
        0x05 => "Memory".to_string(),
        0x06 => "Bridge".to_string(),
        0x07 => "Communication".to_string(),
        0x08 => "System Peripheral".to_string(),
        0x09 => "Input Device".to_string(),
        0x0A => "Docking Station".to_string(),
        0x0B => "Processor".to_string(),
        0x0C => "Serial Bus".to_string(),
        0x0D => "Wireless".to_string(),
        0x0E => "Intelligent I/O".to_string(),
        0x0F => "Satellite Communication".to_string(),
        0x10 => "Encryption/Decryption".to_string(),
        0x11 => "Signal Processing".to_string(),
        0x12 => "Processing Accelerator".to_string(),
        0x13 => "Non-Essential Instrumentation".to_string(),
        0x40 => "Co-Processor".to_string(),
        _ => format!("Class 0x{:02X}", class_id),
    }
}

/// Get SATA/storage devices from /sys/class/block
pub fn get_sata_devices() -> Result<Vec<SataDeviceInfo>, Error> {
    let block_dir = Path::new("/sys/class/block");
    if !block_dir.exists() {
        return Ok(Vec::new());
    }

    let mut devices = Vec::new();

    for entry in fs::read_dir(block_dir).map_err(Error::IoError)? {
        let entry = entry.map_err(Error::IoError)?;
        let name = entry.file_name().to_string_lossy().to_string();

        // Skip partitions (sda1, nvme0n1p1) - only include whole disks
        if name.contains('p') && name.starts_with("nvme") {
            continue; // Skip NVMe partition
        }
        if name.chars().last().is_some_and(|c| c.is_ascii_digit()) && !name.starts_with("nvme") {
            // Check if it's a partition of sdX type
            let base: String = name.chars().take_while(|c| !c.is_ascii_digit()).collect();
            if base.len() < name.len()
                && (base.starts_with("sd") || base.starts_with("hd") || base.starts_with("vd"))
            {
                continue;
            }
        }

        let path = entry.path();
        let device_path = path.join("device");

        // Skip non-physical devices (loop, ram, dm)
        if name.starts_with("loop") || name.starts_with("ram") || name.starts_with("dm-") {
            continue;
        }

        // Read model
        let model = fs::read_to_string(device_path.join("model"))
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        // Read serial
        let serial = fs::read_to_string(device_path.join("serial"))
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        // Read firmware
        let firmware = fs::read_to_string(device_path.join("firmware_rev"))
            .or_else(|_| fs::read_to_string(device_path.join("rev")))
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        // Read size (in 512-byte sectors)
        let capacity_gb = fs::read_to_string(path.join("size"))
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .map(|sectors| (sectors * 512) as f64 / (1024.0 * 1024.0 * 1024.0));

        // Determine rotation rate to classify HDD vs SSD
        let rotation = fs::read_to_string(path.join("queue/rotational"))
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok());
        let media_type = match rotation {
            Some(0) => SataMediaType::Ssd,
            Some(1) => SataMediaType::Hdd,
            _ => SataMediaType::Unknown,
        };

        // Read temperature from hwmon if available
        let temperature = fs::read_dir(device_path.join("hwmon"))
            .ok()
            .and_then(|mut entries| entries.next())
            .and_then(|entry| entry.ok())
            .and_then(|entry| {
                fs::read_to_string(entry.path().join("temp1_input"))
                    .ok()
                    .and_then(|s| s.trim().parse::<f32>().ok())
                    .map(|t| t / 1000.0)
            });

        // Interface speed
        let interface_speed = if name.starts_with("nvme") {
            Some("NVMe".to_string())
        } else {
            // Try to detect SATA generation from link speed
            fs::read_to_string(format!("/sys/class/ata_link/link{}/sata_spd", 1))
                .ok()
                .map(|s| {
                    let s = s.trim();
                    match s {
                        "6.0 Gbps" | "6" => "SATA III (6 Gbps)".to_string(),
                        "3.0 Gbps" | "3" => "SATA II (3 Gbps)".to_string(),
                        "1.5 Gbps" | "1.5" => "SATA I (1.5 Gbps)".to_string(),
                        _ => format!("SATA ({})", s),
                    }
                })
        };

        devices.push(SataDeviceInfo {
            name,
            model,
            serial,
            firmware,
            capacity_gb,
            interface_speed,
            port: None,
            temperature,
            media_type,
        });
    }

    Ok(devices)
}

/// Get system temperatures from hwmon
pub fn get_system_temperatures() -> Result<SystemTemperatures, Error> {
    let sensors = enumerate()?;

    let mut cpu_temp: Option<f32> = None;
    let mut mb_temp: Option<f32> = None;
    let mut storage_temps: Vec<(String, f32)> = Vec::new();

    for sensor in &sensors {
        if let Ok(temp_sensors) = sensor.temperature_sensors() {
            for ts in &temp_sensors {
                match ts.sensor_type {
                    SensorType::Cpu => {
                        // Highest CPU temperature among sensors that produced
                        // one. A sensor with no reading cannot be the hottest.
                        if let Some(celsius) = ts.temperature {
                            if cpu_temp.is_none_or(|t| celsius > t) {
                                cpu_temp = Some(celsius);
                            }
                        }
                    }
                    SensorType::Chipset | SensorType::Ambient | SensorType::Vrm => {
                        if let Some(celsius) = ts.temperature {
                            if mb_temp.is_none_or(|t| celsius > t) {
                                mb_temp = Some(celsius);
                            }
                        }
                    }
                    SensorType::M2Slot => {
                        if let Some(celsius) = ts.temperature {
                            storage_temps.push((ts.label.clone(), celsius));
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    // Also check block device temperatures
    let block_dir = Path::new("/sys/class/block");
    if block_dir.exists() {
        if let Ok(entries) = fs::read_dir(block_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with("sd") || name.starts_with("nvme") {
                    let hwmon_path = entry.path().join("device/hwmon");
                    if let Ok(mut hwmon_entries) = fs::read_dir(&hwmon_path) {
                        if let Some(Ok(hwmon_entry)) = hwmon_entries.next() {
                            if let Ok(temp_str) =
                                fs::read_to_string(hwmon_entry.path().join("temp1_input"))
                            {
                                if let Ok(temp_millic) = temp_str.trim().parse::<f32>() {
                                    let temp_c = temp_millic / 1000.0;
                                    if temp_c > 0.0 && temp_c < 150.0 {
                                        storage_temps.push((name, temp_c));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(SystemTemperatures {
        cpu: cpu_temp,
        gpu: None, // GPU temps come from GPU module
        motherboard: mb_temp,
        storage: storage_temps,
        network: Vec::new(),
    })
}

/// Get all peripheral devices on Linux
pub fn get_peripherals() -> Result<PeripheralsInfo, Error> {
    let mut info = PeripheralsInfo::default();

    // USB devices from /sys/bus/usb/devices
    let usb_dir = Path::new("/sys/bus/usb/devices");
    if usb_dir.exists() {
        if let Ok(entries) = fs::read_dir(usb_dir) {
            for entry in entries.flatten() {
                let path = entry.path();

                // Only process actual devices (not interfaces)
                let name_str = entry.file_name().to_string_lossy().to_string();
                if name_str.contains(':') {
                    continue; // Skip interface entries
                }

                let product = fs::read_to_string(path.join("product"))
                    .ok()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());
                let manufacturer = fs::read_to_string(path.join("manufacturer"))
                    .ok()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());
                let vendor_id = fs::read_to_string(path.join("idVendor"))
                    .ok()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());
                let product_id = fs::read_to_string(path.join("idProduct"))
                    .ok()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());
                let speed = fs::read_to_string(path.join("speed"))
                    .ok()
                    .and_then(|s| s.trim().parse::<u32>().ok());
                let dev_class = fs::read_to_string(path.join("bDeviceClass"))
                    .ok()
                    .map(|s| s.trim().to_string());

                // Skip root hubs and devices with no product info
                if dev_class.as_deref() == Some("09") {
                    continue; // Hub
                }

                let name = product.unwrap_or_else(|| format!("USB Device {}", name_str));

                let usb_version = match speed {
                    Some(s) if s >= 10000 => UsbVersion::Usb3_1,
                    Some(s) if s >= 5000 => UsbVersion::Usb3_0,
                    Some(s) if s >= 480 => UsbVersion::Usb2_0,
                    Some(s) if s >= 12 => UsbVersion::Usb1_1,
                    _ => UsbVersion::Unknown,
                };

                info.usb_devices.push(UsbDeviceInfo {
                    name,
                    device_id: None,
                    vendor: manufacturer,
                    product_id,
                    vendor_id,
                    usb_version,
                    device_class: dev_class,
                    status: Some("Connected".to_string()),
                    hub_port: None,
                });
            }
        }
    }

    // Display outputs from /sys/class/drm
    let drm_dir = Path::new("/sys/class/drm");
    if drm_dir.exists() {
        if let Ok(entries) = fs::read_dir(drm_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                // Only connector entries (card0-HDMI-A-1, card0-DP-1, etc)
                if !name.contains('-') || name == "version" {
                    continue;
                }

                let status = fs::read_to_string(entry.path().join("status"))
                    .ok()
                    .map(|s| s.trim().to_string());

                let output_type = if name.contains("HDMI") {
                    DisplayOutputType::Hdmi
                // eDP must be tested before DP: "eDP-1" contains "DP", so the
                // DisplayPort arm would claim every internal panel and the eDP arm
                // below could never be reached. The enum has no `Edp`; an embedded
                // DisplayPort *is* the internal panel, which is what `Internal`
                // means.
                } else if name.contains("eDP") {
                    DisplayOutputType::Internal
                } else if name.contains("DP") || name.contains("DisplayPort") {
                    DisplayOutputType::DisplayPort
                } else if name.contains("VGA") {
                    DisplayOutputType::Vga
                } else if name.contains("DVI") {
                    DisplayOutputType::Dvi
                } else {
                    DisplayOutputType::Unknown
                };

                info.display_outputs.push(DisplayOutputInfo {
                    name: name.clone(),
                    output_type,
                    connected: status.as_deref() == Some("connected"),
                    resolution: None,
                    refresh_rate: None,
                    // sysfs exposes the connector, not which GPU drives it; that
                    // needs a DRM query this path does not make.
                    adapter: None,
                });
            }
        }
    }

    // Audio devices from /proc/asound/cards
    if let Ok(content) = fs::read_to_string("/proc/asound/cards") {
        for line in content.lines() {
            let line = line.trim();
            // Lines like: " 0 [HDA-Intel     ]: HDA-Intel - HDA NVidia"
            if line.starts_with(|c: char| c.is_ascii_digit()) {
                let parts: Vec<&str> = line.splitn(2, ':').collect();
                if parts.len() == 2 {
                    let name = parts[1].trim().to_string();
                    info.audio_devices.push(AudioDeviceInfo {
                        name,
                        device_type: AudioDeviceType::Output,
                        // The struct has `manufacturer`, not `driver`. ALSA is the
                        // subsystem this was read through, not who made the device,
                        // so claiming it as the manufacturer would be wrong.
                        manufacturer: None,
                        status: Some("Available".to_string()),
                        is_default: false,
                    });
                }
            }
        }
    }

    // Bluetooth from /sys/class/bluetooth
    let bt_dir = Path::new("/sys/class/bluetooth");
    if bt_dir.exists() {
        if let Ok(entries) = fs::read_dir(bt_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                let address = fs::read_to_string(entry.path().join("address"))
                    .ok()
                    .map(|s| s.trim().to_string());
                info.bluetooth_devices.push(BluetoothDeviceInfo {
                    name: format!("Bluetooth Adapter ({})", name),
                    address,
                    connected: true,
                    device_type: Some("Adapter".to_string()),
                    // This enumerates adapters under /sys/class/bluetooth, not
                    // remote devices; an adapter is not paired with anything.
                    paired: false,
                });
            }
        }
    }

    // Network ports from /sys/class/net
    let net_dir = Path::new("/sys/class/net");
    if net_dir.exists() {
        if let Ok(entries) = fs::read_dir(net_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name == "lo" {
                    continue; // Skip loopback
                }

                let speed = fs::read_to_string(entry.path().join("speed"))
                    .ok()
                    .and_then(|s| s.trim().parse::<u32>().ok());
                let operstate = fs::read_to_string(entry.path().join("operstate"))
                    .ok()
                    .map(|s| s.trim().to_string());
                let port_type = if name.starts_with("wl") || name.starts_with("wlan") {
                    NetworkPortType::WiFi
                } else if name.starts_with("eth") || name.starts_with("en") {
                    NetworkPortType::Ethernet
                } else {
                    NetworkPortType::Other
                };

                info.network_ports.push(NetworkPortInfo {
                    name,
                    port_type,
                    // The field is `speed: Option<String>`, and sysfs reports Mbit/s;
                    // keep the unit in the text rather than dropping it silently.
                    speed: speed.map(|mbps| format!("{mbps} Mb/s")),
                    connected: operstate.as_deref() == Some("up"),
                    mac_address: fs::read_to_string(entry.path().join("address"))
                        .ok()
                        .map(|s| s.trim().to_string()),
                });
            }
        }
    }

    Ok(info)
}
