// Windows motherboard and system monitoring implementation
//
// Data sources:
// - WMI Win32_BaseBoard - Motherboard information
// - WMI Win32_BIOS - BIOS information
// - WMI Win32_OperatingSystem - OS information
// - WMI Win32_ComputerSystem - System information
// - WMI Win32_PnPSignedDriver - Driver information
// - WMI MSAcpi_ThermalZoneTemperature - ACPI temperatures (if available)
// - WMI Win32_Fan - Fan information (if available)
// - WMI Win32_Processor - CPU information
// - LibreHardwareMonitor/OpenHardwareMonitor WMI (if running)

use super::traits::*;
use serde::Deserialize;
use wmi::{COMLibrary, WMIConnection};

/// Initialize COM and create WMI connection - more robust version
fn create_wmi_connection() -> Result<WMIConnection, Error> {
    // Try multiple strategies to get a working WMI connection

    // Strategy 1: Fresh COM initialization (works best in background threads)
    if let Ok(com) = COMLibrary::new() {
        if let Ok(conn) = WMIConnection::new(com) {
            return Ok(conn);
        }
    }

    // Strategy 2: COM without security init
    if let Ok(com) = COMLibrary::without_security() {
        if let Ok(conn) = WMIConnection::new(com) {
            return Ok(conn);
        }
    }

    // Strategy 3: Assume COM is already initialized by the runtime
    let com = unsafe { COMLibrary::assume_initialized() };
    WMIConnection::new(com).map_err(|e| Error::InitializationFailed(e.to_string()))
}

/// Create WMI connection to a specific namespace
fn create_wmi_connection_namespace(namespace: &str) -> Result<WMIConnection, Error> {
    // Strategy 1: Fresh COM initialization
    if let Ok(com) = COMLibrary::new() {
        if let Ok(conn) = WMIConnection::with_namespace_path(namespace, com) {
            return Ok(conn);
        }
    }

    // Strategy 2: COM without security init
    if let Ok(com) = COMLibrary::without_security() {
        if let Ok(conn) = WMIConnection::with_namespace_path(namespace, com) {
            return Ok(conn);
        }
    }

    // Strategy 3: Assume COM is already initialized
    let com = unsafe { COMLibrary::assume_initialized() };
    WMIConnection::with_namespace_path(namespace, com)
        .map_err(|e| Error::InitializationFailed(e.to_string()))
}

/// LibreHardwareMonitor/OpenHardwareMonitor WMI sensor structure
#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
#[allow(dead_code)]
struct LhmSensor {
    name: Option<String>,
    identifier: Option<String>,
    sensor_type: Option<String>,
    value: Option<f32>,
    min: Option<f32>,
    max: Option<f32>,
    parent: Option<String>,
}

/// LibreHardwareMonitor/OpenHardwareMonitor WMI hardware structure
#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
#[allow(dead_code)]
struct LhmHardware {
    name: Option<String>,
    identifier: Option<String>,
    hardware_type: Option<String>,
    parent: Option<String>,
}

/// Query LibreHardwareMonitor WMI for sensors (if LHM is running)
fn query_lhm_sensors() -> Option<Vec<TemperatureSensor>> {
    // Try LibreHardwareMonitor namespace first
    let namespaces = ["root\\LibreHardwareMonitor", "root\\OpenHardwareMonitor"];

    for namespace in namespaces {
        if let Ok(conn) = create_wmi_connection_namespace(namespace) {
            // Query temperature sensors
            if let Ok(sensors) = conn.raw_query::<LhmSensor>(
                "SELECT Name, Identifier, SensorType, Value, Min, Max, Parent FROM Sensor WHERE SensorType='Temperature'",
            ) {
                if !sensors.is_empty() {
                    let mut temps = Vec::new();
                    for sensor in sensors {
                        if let (Some(name), Some(value)) = (sensor.name, sensor.value) {
                            // Skip invalid readings
                            if value > 0.0 && value < 150.0 {
                                // Determine sensor type from identifier
                                let sensor_type = sensor
                                    .identifier
                                    .as_ref()
                                    .map(|id| {
                                        if id.contains("/cpu") {
                                            SensorType::Cpu
                                        } else if id.contains("/gpu") {
                                            SensorType::Gpu
                                        } else if id.contains("/hdd") || id.contains("/nvme") {
                                            SensorType::Storage
                                        } else {
                                            SensorType::Other
                                        }
                                    })
                                    .unwrap_or(SensorType::Other);

                                temps.push(TemperatureSensor {
                                    label: name,
                                    // A real reading from the provider.
                                    temperature: Some(value),
                                    max: sensor.max,
                                    critical: None,
                                    sensor_type,
                                });
                            }
                        }
                    }
                    if !temps.is_empty() {
                        return Some(temps);
                    }
                }
            }
        }
    }
    None
}

/// Query LibreHardwareMonitor for voltage sensors
fn query_lhm_voltages() -> Option<Vec<VoltageRail>> {
    let namespaces = ["root\\LibreHardwareMonitor", "root\\OpenHardwareMonitor"];

    for namespace in namespaces {
        if let Ok(conn) = create_wmi_connection_namespace(namespace) {
            if let Ok(sensors) = conn.raw_query::<LhmSensor>(
                "SELECT Name, Value, Min, Max FROM Sensor WHERE SensorType='Voltage'",
            ) {
                if !sensors.is_empty() {
                    let mut voltages = Vec::new();
                    for sensor in sensors {
                        if let (Some(name), Some(value)) = (sensor.name, sensor.value) {
                            if value > 0.0 && value < 20.0 {
                                voltages.push(VoltageRail {
                                    label: name,
                                    voltage: Some(value),
                                    min: sensor.min,
                                    max: sensor.max,
                                });
                            }
                        }
                    }
                    if !voltages.is_empty() {
                        return Some(voltages);
                    }
                }
            }
        }
    }
    None
}

/// Query LibreHardwareMonitor for fan sensors
fn query_lhm_fans() -> Option<Vec<FanInfo>> {
    let namespaces = ["root\\LibreHardwareMonitor", "root\\OpenHardwareMonitor"];

    for namespace in namespaces {
        if let Ok(conn) = create_wmi_connection_namespace(namespace) {
            if let Ok(sensors) = conn.raw_query::<LhmSensor>(
                "SELECT Name, Value, Min, Max FROM Sensor WHERE SensorType='Fan'",
            ) {
                if !sensors.is_empty() {
                    let mut fans = Vec::new();
                    for sensor in sensors {
                        if let Some(name) = sensor.name {
                            fans.push(FanInfo {
                                label: name,
                                rpm: sensor.value.map(|v| v as u32),
                                pwm: None,
                                min_rpm: sensor.min.map(|v| v as u32),
                                max_rpm: sensor.max.map(|v| v as u32),
                                controllable: false,
                            });
                        }
                    }
                    if !fans.is_empty() {
                        return Some(fans);
                    }
                }
            }
        }
    }
    None
}

/// Check if LibreHardwareMonitor or OpenHardwareMonitor is running
fn is_lhm_available() -> bool {
    let namespaces = ["root\\LibreHardwareMonitor", "root\\OpenHardwareMonitor"];

    for namespace in namespaces {
        if create_wmi_connection_namespace(namespace).is_ok() {
            return true;
        }
    }
    false
}

/// WMI structures for deserialization

#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
struct Win32BaseBoard {
    manufacturer: Option<String>,
    product: Option<String>,
    version: Option<String>,
    serial_number: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
struct Win32Bios {
    manufacturer: Option<String>,
    #[serde(rename = "SMBIOSBIOSVersion")]
    smbios_bios_version: Option<String>,
    release_date: Option<String>,
    #[serde(rename = "SystemBiosMajorVersion")]
    system_bios_major_version: Option<u8>,
    #[serde(rename = "SystemBiosMinorVersion")]
    system_bios_minor_version: Option<u8>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
struct Win32OperatingSystem {
    caption: Option<String>,
    version: Option<String>,
    build_number: Option<String>,
    #[serde(rename = "OSArchitecture")]
    os_architecture: Option<String>,
    #[serde(rename = "CSName")]
    cs_name: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
struct Win32ComputerSystem {
    manufacturer: Option<String>,
    model: Option<String>,
    system_type: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
struct Win32Processor {
    name: Option<String>,
    number_of_cores: Option<u32>,
    number_of_logical_processors: Option<u32>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
struct Win32ComputerSystemProduct {
    #[serde(rename = "UUID")]
    uuid: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(rename = "MSAcpi_ThermalZoneTemperature")]
#[serde(rename_all = "PascalCase")]
#[allow(dead_code)]
struct MsAcpiThermalZoneTemperature {
    instance_name: Option<String>,
    current_temperature: Option<u32>, // In tenths of Kelvin
    active: Option<bool>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
#[allow(dead_code)]
struct Win32PnPSignedDriver {
    device_name: Option<String>,
    driver_version: Option<String>,
    device_class: Option<String>,
    manufacturer: Option<String>,
    driver_date: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
#[allow(dead_code)]
struct Win32Fan {
    name: Option<String>,
    device_id: Option<String>,
    status: Option<String>,
    active_cooling: Option<bool>,
}

/// WMI structure for PCI devices
#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
struct Win32PnPEntity {
    name: Option<String>,
    device_id: Option<String>,
    manufacturer: Option<String>,
    #[serde(rename = "PNPClass")]
    pnp_class: Option<String>,
    status: Option<String>,
}

/// WMI structure for disk drives
#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
struct Win32DiskDrive {
    caption: Option<String>,
    model: Option<String>,
    serial_number: Option<String>,
    firmware_revision: Option<String>,
    size: Option<u64>,
    interface_type: Option<String>,
    media_type: Option<String>,
    index: Option<u32>,
}

/// WMI structure for disk drive temperatures (via SMART if available)
#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
#[allow(dead_code)]
struct MSStorageDriverFailurePredictData {
    instance_name: Option<String>,
    vendor_specific: Option<Vec<u8>>,
}

/// Windows motherboard sensor implementation using WMI
/// Note: WMI queries are performed on-demand since WMIConnection is not Send + Sync
pub struct WindowsSensor {
    name: String,
}

impl WindowsSensor {
    pub fn new(name: String) -> Self {
        Self { name }
    }
}

impl MotherboardDevice for WindowsSensor {
    fn name(&self) -> &str {
        &self.name
    }

    fn device_path(&self) -> Option<String> {
        if is_lhm_available() {
            Some("LibreHardwareMonitor / OpenHardwareMonitor WMI".to_string())
        } else {
            Some("WMI root\\WMI + root\\CIMV2".to_string())
        }
    }

    fn temperature_sensors(&self) -> Result<Vec<TemperatureSensor>, Error> {
        // First, try LibreHardwareMonitor/OpenHardwareMonitor if running
        if let Some(lhm_temps) = query_lhm_sensors() {
            return Ok(lhm_temps);
        }

        let mut sensors = Vec::new();

        // First, try Win32_TemperatureProbe in root\CIMV2 (doesn't require admin)
        if let Ok(wmi_cimv2) = create_wmi_connection() {
            // Win32_TemperatureProbe - standard WMI class
            #[derive(Deserialize, Debug)]
            #[serde(rename_all = "PascalCase")]
            #[allow(dead_code)]
            struct Win32TemperatureProbe {
                description: Option<String>,
                current_reading: Option<i32>, // In tenths of degrees Celsius
                nominal_reading: Option<i32>,
                status: Option<String>,
            }

            if let Ok(probes) = wmi_cimv2.raw_query::<Win32TemperatureProbe>(
                "SELECT Description, CurrentReading, NominalReading, Status FROM Win32_TemperatureProbe",
            ) {
                for probe in probes {
                    if let Some(temp_tenths) = probe.current_reading {
                        // Convert from tenths of Celsius
                        let temp_celsius = temp_tenths as f32 / 10.0;
                        if temp_celsius > 0.0 && temp_celsius < 150.0 {
                            sensors.push(TemperatureSensor {
                                label: probe
                                    .description
                                    .unwrap_or_else(|| "Temperature Probe".to_string()),
                                temperature: Some(temp_celsius),
                                max: probe.nominal_reading.map(|r| r as f32 / 10.0),
                                critical: None,
                                sensor_type: SensorType::Other,
                            });
                        }
                    }
                }
            }
        }

        // Try WMI with root\WMI namespace for ACPI thermal zones (may require admin)
        let Ok(_base_conn) = create_wmi_connection() else {
            return Ok(sensors);
        };
        // Drop base_conn as we need a different namespace
        drop(_base_conn);

        // We need to get the COM library to create a namespace-specific connection
        // Since create_wmi_connection succeeded, we can try assume_initialized
        let com = unsafe { COMLibrary::assume_initialized() };
        let Ok(wmi_conn) = WMIConnection::with_namespace_path("root\\WMI", com) else {
            return Ok(sensors);
        };

        // Query MSAcpi_ThermalZoneTemperature (requires admin and WMI namespace root\WMI)
        match wmi_conn.raw_query::<MsAcpiThermalZoneTemperature>(
            "SELECT InstanceName, CurrentTemperature, Active FROM MSAcpi_ThermalZoneTemperature",
        ) {
            Ok(thermal_zones) => {
                for zone in thermal_zones {
                    if let Some(temp_decikelvin) = zone.current_temperature {
                        // Convert from decikelvin to Celsius: (K/10 - 273.15)
                        let temp_celsius = (temp_decikelvin as f64 / 10.0) - 273.15;

                        // Skip invalid readings (often 0 or very low values)
                        if temp_celsius > 0.0 && temp_celsius < 150.0 {
                            let name = zone
                                .instance_name
                                .unwrap_or_else(|| "ThermalZone".to_string());

                            sensors.push(TemperatureSensor {
                                label: format!("ACPI {}", name),
                                temperature: Some(temp_celsius as f32),
                                max: Some(90.0),       // Typical high threshold
                                critical: Some(105.0), // Typical critical threshold
                                sensor_type: SensorType::Other, // ACPI thermal zone
                            });
                        }
                    }
                }
            }
            Err(_e) => {
                // MSAcpi thermal zones often require admin privileges
                // Silently ignore - this is expected on most systems without admin
            }
        }

        Ok(sensors)
    }

    fn voltage_rails(&self) -> Result<Vec<VoltageRail>, Error> {
        // Try LibreHardwareMonitor/OpenHardwareMonitor first
        if let Some(lhm_voltages) = query_lhm_voltages() {
            return Ok(lhm_voltages);
        }

        // WMI doesn't provide voltage information directly
        // Would need LibreHardwareMonitor or OpenHardwareMonitor integration
        Ok(Vec::new())
    }

    fn fans(&self) -> Result<Vec<FanInfo>, Error> {
        // Try LibreHardwareMonitor/OpenHardwareMonitor first
        if let Some(lhm_fans) = query_lhm_fans() {
            return Ok(lhm_fans);
        }

        // Fall back to Win32_Fan
        let Ok(wmi_cimv2) = create_wmi_connection() else {
            return Ok(Vec::new());
        };

        let mut fans = Vec::new();

        // Query Win32_Fan (rarely populated on most systems)
        match wmi_cimv2
            .raw_query::<Win32Fan>("SELECT Name, DeviceID, Status, ActiveCooling FROM Win32_Fan")
        {
            Ok(wmi_fans) => {
                for fan in wmi_fans {
                    fans.push(FanInfo {
                        label: fan.name.unwrap_or_else(|| "System Fan".to_string()),
                        rpm: None, // Win32_Fan doesn't provide RPM
                        pwm: None,
                        min_rpm: None,
                        max_rpm: None,
                        controllable: false, // WMI doesn't support fan control
                    });
                }
            }
            Err(_e) => {
                // Win32_Fan is rarely populated on most systems - silently ignore
            }
        }

        Ok(fans)
    }
}

/// Enumerate motherboard sensors
pub fn enumerate() -> Result<Vec<Box<dyn MotherboardDevice>>, Error> {
    let mut devices: Vec<Box<dyn MotherboardDevice>> = Vec::new();

    // Get board name for the sensor
    let board_name = get_board_name().unwrap_or_else(|| "System Board".to_string());
    devices.push(Box::new(WindowsSensor::new(board_name)));

    Ok(devices)
}

/// Get the motherboard name from WMI
fn get_board_name() -> Option<String> {
    let wmi_conn = create_wmi_connection().ok()?;

    let boards: Vec<Win32BaseBoard> = wmi_conn
        .raw_query("SELECT Manufacturer, Product FROM Win32_BaseBoard")
        .ok()?;

    boards
        .first()
        .and_then(|b| match (&b.manufacturer, &b.product) {
            (Some(mfr), Some(prod)) => Some(format!("{} {}", mfr, prod)),
            (None, Some(prod)) => Some(prod.clone()),
            (Some(mfr), None) => Some(mfr.clone()),
            (None, None) => None,
        })
}

/// Get system information via WMI
pub fn get_system_info() -> Result<SystemInfo, Error> {
    let wmi_conn = create_wmi_connection()?;

    // Query Win32_OperatingSystem
    let os_info: Option<Win32OperatingSystem> = wmi_conn
        .raw_query::<Win32OperatingSystem>(
            "SELECT Caption, Version, BuildNumber, OSArchitecture, CSName FROM Win32_OperatingSystem",
        )
        .ok()
        .and_then(|v: Vec<Win32OperatingSystem>| v.into_iter().next());

    // Query Win32_BIOS
    let bios_info: Option<Win32Bios> = wmi_conn
        .raw_query::<Win32Bios>(
            "SELECT Manufacturer, SMBIOSBIOSVersion, ReleaseDate, SystemBiosMajorVersion, SystemBiosMinorVersion FROM Win32_BIOS",
        )
        .ok()
        .and_then(|v: Vec<Win32Bios>| v.into_iter().next());

    // Query Win32_ComputerSystem
    let cs_info: Option<Win32ComputerSystem> = wmi_conn
        .raw_query::<Win32ComputerSystem>(
            "SELECT Manufacturer, Model, SystemType FROM Win32_ComputerSystem",
        )
        .ok()
        .and_then(|v: Vec<Win32ComputerSystem>| v.into_iter().next());

    // Query Win32_BaseBoard
    let bb_info: Option<Win32BaseBoard> = wmi_conn
        .raw_query::<Win32BaseBoard>(
            "SELECT Manufacturer, Product, Version, SerialNumber FROM Win32_BaseBoard",
        )
        .ok()
        .and_then(|v: Vec<Win32BaseBoard>| v.into_iter().next());

    // Query Win32_Processor
    let proc_info: Option<Win32Processor> = wmi_conn
        .raw_query::<Win32Processor>(
            "SELECT Name, NumberOfCores, NumberOfLogicalProcessors FROM Win32_Processor",
        )
        .ok()
        .and_then(|v: Vec<Win32Processor>| v.into_iter().next());

    // Query Win32_ComputerSystemProduct for UUID
    let csp_info: Option<Win32ComputerSystemProduct> = wmi_conn
        .raw_query::<Win32ComputerSystemProduct>("SELECT UUID FROM Win32_ComputerSystemProduct")
        .ok()
        .and_then(|v: Vec<Win32ComputerSystemProduct>| v.into_iter().next());

    // Build BIOS info
    let bios = BiosInfo {
        vendor: bios_info.as_ref().and_then(|b| b.manufacturer.clone()),
        version: bios_info
            .as_ref()
            .and_then(|b| b.smbios_bios_version.clone()),
        release_date: bios_info.as_ref().and_then(|b| {
            // WMI date format: YYYYMMDDHHMMSS.mmmmmm+UUU
            b.release_date.as_ref().map(|d| {
                if d.len() >= 8 {
                    format!("{}-{}-{}", &d[0..4], &d[4..6], &d[6..8])
                } else {
                    d.clone()
                }
            })
        }),
        revision: bios_info.as_ref().and_then(|b| {
            match (b.system_bios_major_version, b.system_bios_minor_version) {
                (Some(major), Some(minor)) => Some(format!("{}.{}", major, minor)),
                _ => None,
            }
        }),
        firmware_type: detect_firmware_type(),
        secure_boot: detect_secure_boot(),
    };

    // Build architecture from Win32_OperatingSystem or Win32_ComputerSystem
    let architecture = os_info
        .as_ref()
        .and_then(|o| o.os_architecture.clone())
        .or_else(|| cs_info.as_ref().and_then(|c| c.system_type.clone()))
        .unwrap_or_else(|| std::env::consts::ARCH.to_string());

    // Build OS version string
    let os_version = os_info
        .as_ref()
        .and_then(|o| match (&o.version, &o.build_number) {
            (Some(ver), Some(build)) => Some(format!("{} (Build {})", ver, build)),
            (Some(ver), None) => Some(ver.clone()),
            _ => None,
        })
        .unwrap_or_else(|| "Unknown".to_string());

    Ok(SystemInfo {
        os_name: os_info
            .as_ref()
            .and_then(|o| o.caption.clone())
            .unwrap_or_else(|| "Windows".to_string()),
        os_version,
        kernel_version: os_info.as_ref().and_then(|o| o.version.clone()),
        architecture,
        hostname: os_info.as_ref().and_then(|o| o.cs_name.clone()),
        bios,
        manufacturer: cs_info.as_ref().and_then(|c| c.manufacturer.clone()),
        product_name: cs_info.as_ref().and_then(|c| c.model.clone()),
        serial_number: bb_info.as_ref().and_then(|b| b.serial_number.clone()),
        uuid: csp_info.and_then(|c| c.uuid),
        board_vendor: bb_info.as_ref().and_then(|b| b.manufacturer.clone()),
        board_name: bb_info.as_ref().and_then(|b| b.product.clone()),
        board_version: bb_info.as_ref().and_then(|b| b.version.clone()),
        cpu_name: proc_info.as_ref().and_then(|p| p.name.clone()),
        cpu_cores: proc_info.as_ref().and_then(|p| p.number_of_cores),
        cpu_threads: proc_info
            .as_ref()
            .and_then(|p| p.number_of_logical_processors),
    })
}

/// Detect firmware type (UEFI vs Legacy BIOS)
fn detect_firmware_type() -> FirmwareType {
    // Check for EFI system partition via environment variable
    if std::env::var("EFI_SYSTEM_PARTITION").is_ok() {
        return FirmwareType::Uefi;
    }

    // Check via registry (more reliable)
    use winreg::enums::*;
    use winreg::RegKey;

    if RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey("SYSTEM\\CurrentControlSet\\Control\\SecureBoot\\State")
        .is_ok()
    {
        // If SecureBoot key exists, it's UEFI
        return FirmwareType::Uefi;
    }

    // Alternative check: look for EFI partition
    if std::path::Path::new("C:\\EFI").exists() {
        return FirmwareType::Uefi;
    }

    // Another method: check for EFI in bcdedit output (would require admin)
    // For now, assume UEFI on modern Windows as it's most common
    FirmwareType::Uefi
}

/// Detect Secure Boot status
fn detect_secure_boot() -> Option<bool> {
    use winreg::enums::*;
    use winreg::RegKey;

    // Try to read Secure Boot state from registry
    if let Ok(hklm) = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey("SYSTEM\\CurrentControlSet\\Control\\SecureBoot\\State")
    {
        if let Ok(value) = hklm.get_value::<u32, _>("UEFISecureBootEnabled") {
            return Some(value != 0);
        }
    }

    None
}

/// Get driver versions via WMI
/// Kernel-mode drivers currently in the running state, as `(name, state)` pairs.
///
/// One WMI query over the shared COM connection. The caller previously spawned
/// PowerShell to run the same query and emit JSON, which cost roughly a second.
pub fn get_running_drivers() -> Result<Vec<(String, String)>, Error> {
    #[derive(Deserialize, Debug)]
    #[serde(rename_all = "PascalCase")]
    struct Win32SystemDriver {
        name: Option<String>,
        state: Option<String>,
    }

    let wmi_conn = create_wmi_connection()?;
    let rows: Vec<Win32SystemDriver> = wmi_conn
        .raw_query("SELECT Name, State FROM Win32_SystemDriver WHERE State = 'Running'")
        .map_err(|e| Error::QueryFailed(e.to_string()))?;

    Ok(rows
        .into_iter()
        .filter_map(|d| {
            let name = d.name?;
            // A driver with no name identifies nothing, so it is dropped rather than
            // listed as an empty entry.
            if name.trim().is_empty() {
                return None;
            }
            Some((name, d.state.unwrap_or_default()))
        })
        .collect())
}

pub fn get_driver_versions() -> Result<Vec<DriverInfo>, Error> {
    let wmi_conn = create_wmi_connection()?;

    let mut drivers = Vec::new();

    // Query specific driver classes that are most useful
    let driver_classes = ["Display", "Net", "DiskDrive", "USB", "Processor"];

    for class in driver_classes {
        let query = format!(
            "SELECT DeviceName, DriverVersion, DeviceClass, Manufacturer, DriverDate FROM Win32_PnPSignedDriver WHERE DeviceClass = '{}'",
            class
        );

        match wmi_conn.raw_query::<Win32PnPSignedDriver>(&query) {
            Ok(class_drivers) => {
                for driver in class_drivers {
                    if let Some(name) = driver.device_name {
                        let driver_type = match class {
                            "Display" => DriverType::Gpu,
                            "Net" => DriverType::Network,
                            "DiskDrive" => DriverType::Storage,
                            "USB" => DriverType::Usb,
                            "Processor" => DriverType::Chipset,
                            _ => DriverType::Other,
                        };

                        // Parse WMI date format to readable format
                        let date = driver.driver_date.map(|d| {
                            if d.len() >= 8 {
                                format!("{}-{}-{}", &d[0..4], &d[4..6], &d[6..8])
                            } else {
                                d
                            }
                        });

                        drivers.push(DriverInfo {
                            name,
                            version: driver
                                .driver_version
                                .unwrap_or_else(|| "Unknown".to_string()),
                            driver_type,
                            description: None,
                            vendor: driver.manufacturer,
                            date,
                        });
                    }
                }
            }
            Err(_e) => {
                // Some driver classes may not exist - silently ignore
            }
        }
    }

    // Also get any driver with NVIDIA, AMD, or Intel in the name (GPU drivers)
    let gpu_query = "SELECT DeviceName, DriverVersion, DeviceClass, Manufacturer, DriverDate FROM Win32_PnPSignedDriver WHERE DeviceName LIKE '%NVIDIA%' OR DeviceName LIKE '%AMD%' OR DeviceName LIKE '%Intel%'";

    if let Ok(gpu_drivers) = wmi_conn.raw_query::<Win32PnPSignedDriver>(gpu_query) {
        for driver in gpu_drivers {
            if let Some(name) = driver.device_name {
                // Skip if we already have this driver
                if drivers.iter().any(|d| d.name == name) {
                    continue;
                }

                drivers.push(DriverInfo {
                    name,
                    version: driver
                        .driver_version
                        .unwrap_or_else(|| "Unknown".to_string()),
                    driver_type: DriverType::Gpu,
                    description: None,
                    vendor: driver.manufacturer,
                    date: driver.driver_date.map(|d| {
                        if d.len() >= 8 {
                            format!("{}-{}-{}", &d[0..4], &d[4..6], &d[6..8])
                        } else {
                            d
                        }
                    }),
                });
            }
        }
    }

    Ok(drivers)
}

/// Get PCIe devices via WMI
pub fn get_pcie_devices() -> Result<Vec<PcieDeviceInfo>, Error> {
    let wmi_conn = create_wmi_connection()?;
    let mut devices = Vec::new();

    // Query PCI devices that are connected via PCIe
    // Look for devices with PCI in their device ID
    let query = "SELECT Name, DeviceID, Manufacturer, PNPClass, Status FROM Win32_PnPEntity WHERE DeviceID LIKE 'PCI%'";

    if let Ok(pci_devices) = wmi_conn.raw_query::<Win32PnPEntity>(query) {
        for device in pci_devices {
            if let Some(name) = device.name {
                // Parse device ID for vendor/device info
                // Format: PCI\VEN_XXXX&DEV_YYYY&SUBSYS_ZZZZZZZZ&REV_RR
                let (vendor_id, device_id_hex) = if let Some(ref dev_id) = device.device_id {
                    let parts: Vec<&str> = dev_id.split('\\').collect();
                    if parts.len() >= 2 {
                        let id_part = parts[1];
                        let vendor = id_part
                            .split('&')
                            .find(|p| p.starts_with("VEN_"))
                            .map(|v| v.trim_start_matches("VEN_").to_string());
                        let dev = id_part
                            .split('&')
                            .find(|p| p.starts_with("DEV_"))
                            .map(|d| d.trim_start_matches("DEV_").to_string());
                        (vendor, dev)
                    } else {
                        (None, None)
                    }
                } else {
                    (None, None)
                };

                // Determine device class
                let device_class = device.pnp_class.clone().or_else(|| {
                    // Infer from name
                    let name_lower = name.to_lowercase();
                    if name_lower.contains("vga")
                        || name_lower.contains("display")
                        || name_lower.contains("graphics")
                    {
                        Some("Display".to_string())
                    } else if name_lower.contains("ethernet")
                        || name_lower.contains("network")
                        || name_lower.contains("wifi")
                    {
                        Some("Network".to_string())
                    } else if name_lower.contains("nvme")
                        || name_lower.contains("ssd")
                        || name_lower.contains("ahci")
                    {
                        Some("Storage".to_string())
                    } else if name_lower.contains("usb") {
                        Some("USB".to_string())
                    } else if name_lower.contains("audio") || name_lower.contains("sound") {
                        Some("Audio".to_string())
                    } else {
                        None
                    }
                });

                devices.push(PcieDeviceInfo {
                    name,
                    device_id: device_id_hex,
                    vendor: device.manufacturer.or(vendor_id),
                    pcie_version: None, // WMI doesn't directly expose PCIe version
                    link_width: None,   // Would need to query via SetupAPI
                    link_speed: None,
                    slot: None,
                    device_class,
                });
            }
        }
    }

    Ok(devices)
}

/// Get SATA/storage devices via WMI
pub fn get_sata_devices() -> Result<Vec<SataDeviceInfo>, Error> {
    let wmi_conn = create_wmi_connection()?;
    let mut devices = Vec::new();

    // Query disk drives
    let query = "SELECT Caption, Model, SerialNumber, FirmwareRevision, Size, InterfaceType, MediaType, Index FROM Win32_DiskDrive";

    if let Ok(disk_drives) = wmi_conn.raw_query::<Win32DiskDrive>(query) {
        for disk in disk_drives {
            let name = disk.caption.unwrap_or_else(|| {
                disk.model
                    .clone()
                    .unwrap_or_else(|| "Unknown Disk".to_string())
            });

            // Determine if it's SATA based on interface
            let interface = disk.interface_type.as_deref().unwrap_or("");
            let interface_speed = match interface.to_uppercase().as_str() {
                "IDE" | "SCSI" | "SATA" => Some("SATA".to_string()),
                "USB" => Some("USB".to_string()),
                "NVME" => Some("NVMe".to_string()),
                _ => disk.interface_type.clone(),
            };

            // Determine media type
            let media_type = match disk.media_type.as_deref().unwrap_or("") {
                s if s.contains("SSD") || s.contains("Solid") => SataMediaType::Ssd,
                s if s.contains("Fixed") || s.contains("HDD") => {
                    // Check model name for SSD hints
                    let model = disk.model.as_deref().unwrap_or("");
                    if model.to_uppercase().contains("SSD") || model.to_uppercase().contains("NVME")
                    {
                        SataMediaType::Ssd
                    } else {
                        SataMediaType::Hdd
                    }
                }
                _ => SataMediaType::Unknown,
            };

            // Convert size to GB
            let capacity_gb = disk.size.map(|s| s as f64 / 1_000_000_000.0);

            devices.push(SataDeviceInfo {
                name,
                model: disk.model,
                serial: disk.serial_number.map(|s| s.trim().to_string()),
                firmware: disk.firmware_revision,
                capacity_gb,
                interface_speed,
                port: disk.index.map(|i| i as u8),
                temperature: None, // Would need SMART data access
                media_type,
            });
        }
    }

    Ok(devices)
}

/// Get system temperatures from various sources
pub fn get_system_temperatures() -> Result<SystemTemperatures, Error> {
    use crate::hwmon::{self, HwSensorType, HwType};

    let mut cpu_temp: Option<f32> = None;
    let mut gpu_temp: Option<f32> = None;
    let mut mb_temp: Option<f32> = None;
    let mut storage_temps: Vec<(String, f32)> = Vec::new();

    // First, try our native hwmon implementation
    let monitor = hwmon::HardwareMonitor::new();
    let temps = monitor.temperatures();

    for sensor in &temps {
        match (sensor.sensor_type, sensor.hardware_type) {
            (HwSensorType::Temperature, HwType::Cpu) => {
                // Prefer package/Tctl temp, otherwise use first CPU temp
                if cpu_temp.is_none()
                    || sensor.name.to_lowercase().contains("package")
                    || sensor.name.to_lowercase().contains("tctl")
                {
                    cpu_temp = Some(sensor.value);
                }
            }
            (HwSensorType::Temperature, HwType::Gpu) => {
                if gpu_temp.is_none() {
                    gpu_temp = Some(sensor.value);
                }
            }
            (HwSensorType::Temperature, HwType::Storage) => {
                storage_temps.push((sensor.name.clone(), sensor.value));
            }
            (HwSensorType::Temperature, HwType::Motherboard) if mb_temp.is_none() => {
                mb_temp = Some(sensor.value);
            }
            _ => {}
        }
    }

    // If we got data from hwmon, return it
    if cpu_temp.is_some() || gpu_temp.is_some() || !storage_temps.is_empty() {
        return Ok(SystemTemperatures {
            cpu: cpu_temp,
            gpu: gpu_temp,
            motherboard: mb_temp,
            storage: storage_temps,
            network: Vec::new(),
        });
    }

    // Second fallback: try LibreHardwareMonitor/OpenHardwareMonitor if running
    if let Some(lhm_temps) = query_lhm_sensors() {
        for sensor in &lhm_temps {
            match sensor.sensor_type {
                SensorType::Cpu => {
                    // Use first CPU temp or "CPU Package" if available
                    if cpu_temp.is_none()
                        || sensor.label.to_lowercase().contains("package")
                        || sensor.label.to_lowercase().contains("tctl")
                    {
                        // Propagates absence: a sensor that could not be read
                        // leaves the summary without a CPU temperature rather
                        // than reporting one it does not have.
                        cpu_temp = sensor.temperature;
                    }
                }
                SensorType::Gpu => {
                    if gpu_temp.is_none() {
                        gpu_temp = sensor.temperature;
                    }
                }
                SensorType::Storage => {
                    if let Some(celsius) = sensor.temperature {
                        storage_temps.push((sensor.label.clone(), celsius));
                    }
                }
                SensorType::Other
                    // Check for motherboard sensor
                    if (sensor.label.to_lowercase().contains("system")
                        || sensor.label.to_lowercase().contains("motherboard")
                        || sensor.label.to_lowercase().contains("mainboard"))
                        && mb_temp.is_none() => {
                            mb_temp = sensor.temperature;
                        }
                _ => {}
            }
        }

        // If we got data from LHM, return it (still try NVML for GPU if needed)
        if cpu_temp.is_some() || !storage_temps.is_empty() {
            // Still try NVML if no GPU temp from LHM
            if gpu_temp.is_none() {
                #[cfg(feature = "nvidia")]
                {
                    use nvml_wrapper::Nvml;
                    if let Ok(nvml) = Nvml::init() {
                        if let Ok(device) = nvml.device_by_index(0) {
                            if let Ok(temp) = device.temperature(
                                nvml_wrapper::enum_wrappers::device::TemperatureSensor::Gpu,
                            ) {
                                gpu_temp = Some(temp as f32);
                            }
                        }
                    }
                }
            }

            return Ok(SystemTemperatures {
                cpu: cpu_temp,
                gpu: gpu_temp,
                motherboard: mb_temp,
                storage: storage_temps,
                network: Vec::new(),
            });
        }
    }

    // Third fallback: WMI ACPI thermal zones for CPU temp
    if let Ok(_base_conn) = create_wmi_connection() {
        drop(_base_conn);
        let com = unsafe { COMLibrary::assume_initialized() };

        if let Ok(wmi_wmi) = WMIConnection::with_namespace_path("root\\WMI", com) {
            if let Ok(thermal_zones) = wmi_wmi.raw_query::<MsAcpiThermalZoneTemperature>(
                "SELECT InstanceName, CurrentTemperature FROM MSAcpi_ThermalZoneTemperature",
            ) {
                for zone in thermal_zones {
                    if let Some(temp_decikelvin) = zone.current_temperature {
                        let temp_celsius = (temp_decikelvin as f64 / 10.0) - 273.15;
                        if temp_celsius > 0.0 && temp_celsius < 150.0 {
                            // First valid thermal zone is often CPU
                            if cpu_temp.is_none() {
                                cpu_temp = Some(temp_celsius as f32);
                            }
                        }
                    }
                }
            }
        }
    }

    // Try to get GPU temperature from NVML
    #[cfg(feature = "nvidia")]
    {
        use nvml_wrapper::Nvml;
        if let Ok(nvml) = Nvml::init() {
            if let Ok(device) = nvml.device_by_index(0) {
                if let Ok(temp) =
                    device.temperature(nvml_wrapper::enum_wrappers::device::TemperatureSensor::Gpu)
                {
                    gpu_temp = Some(temp as f32);
                }
            }
        }
    }

    Ok(SystemTemperatures {
        cpu: cpu_temp,
        gpu: gpu_temp,
        motherboard: mb_temp,
        storage: storage_temps,
        network: Vec::new(),
    })
}

/// WMI structure for USB devices
#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
struct Win32UsbHub {
    name: Option<String>,
    device_id: Option<String>,
    #[serde(rename = "PNPDeviceID")]
    pnp_device_id: Option<String>,
    status: Option<String>,
}

/// WMI structure for USB controllers
#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
struct Win32UsbController {
    name: Option<String>,
    device_id: Option<String>,
    manufacturer: Option<String>,
    #[serde(rename = "PNPDeviceID")]
    pnp_device_id: Option<String>,
    status: Option<String>,
}

/// WMI structure for video controllers (for display output info)
#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
#[allow(dead_code)]
struct Win32VideoController {
    name: Option<String>,
    adapter_compatibility: Option<String>,
    video_processor: Option<String>,
    current_horizontal_resolution: Option<u32>,
    current_vertical_resolution: Option<u32>,
    current_refresh_rate: Option<u32>,
    status: Option<String>,
}

/// WMI structure for monitors
#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
#[allow(dead_code)]
struct Win32DesktopMonitor {
    name: Option<String>,
    monitor_manufacturer: Option<String>,
    monitor_type: Option<String>,
    screen_width: Option<u32>,
    screen_height: Option<u32>,
}

/// WMI structure for sound devices
#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
#[allow(dead_code)]
struct Win32SoundDevice {
    name: Option<String>,
    manufacturer: Option<String>,
    status: Option<String>,
    device_id: Option<String>,
    #[serde(rename = "PNPDeviceID")]
    pnp_device_id: Option<String>,
}

/// WMI structure for network adapters
#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
#[allow(dead_code)]
struct Win32NetworkAdapterConfig {
    description: Option<String>,
    #[serde(rename = "MACAddress")]
    mac_address: Option<String>,
    #[serde(rename = "IPAddress")]
    ip_address: Option<Vec<String>>,
    #[serde(rename = "IPEnabled")]
    ip_enabled: Option<bool>,
}

/// Get all peripherals information
pub fn get_peripherals() -> Result<PeripheralsInfo, Error> {
    let wmi_conn = create_wmi_connection()?;

    Ok(PeripheralsInfo {
        usb_devices: get_usb_devices(&wmi_conn),
        display_outputs: get_display_outputs(&wmi_conn),
        audio_devices: get_audio_devices(&wmi_conn),
        network_ports: get_network_ports(&wmi_conn),
        ..Default::default()
    })
}

/// Get USB devices from WMI
fn get_usb_devices(wmi_conn: &WMIConnection) -> Vec<UsbDeviceInfo> {
    let mut devices = Vec::new();

    // Query USB controllers to understand USB versions
    let controllers_query =
        "SELECT Name, DeviceID, Manufacturer, PNPDeviceID, Status FROM Win32_USBController";
    if let Ok(controllers) = wmi_conn.raw_query::<Win32UsbController>(controllers_query) {
        for controller in controllers {
            if let Some(name) = controller.name {
                // Determine USB version from controller name
                let usb_version = detect_usb_version(&name);

                devices.push(UsbDeviceInfo {
                    name,
                    device_id: controller.device_id,
                    vendor: controller.manufacturer,
                    product_id: None,
                    vendor_id: extract_vendor_id(&controller.pnp_device_id),
                    usb_version,
                    device_class: Some("Controller".to_string()),
                    status: controller.status,
                    hub_port: None,
                });
            }
        }
    }

    // Query USB hubs
    let hubs_query = "SELECT Name, DeviceID, PNPDeviceID, Status FROM Win32_USBHub";
    if let Ok(hubs) = wmi_conn.raw_query::<Win32UsbHub>(hubs_query) {
        for hub in hubs {
            if let Some(name) = hub.name {
                let usb_version = detect_usb_version(&name);

                devices.push(UsbDeviceInfo {
                    name,
                    device_id: hub.device_id,
                    vendor: None,
                    product_id: None,
                    vendor_id: extract_vendor_id(&hub.pnp_device_id),
                    usb_version,
                    device_class: Some("Hub".to_string()),
                    status: hub.status,
                    hub_port: None,
                });
            }
        }
    }

    // Query connected USB devices via PnP
    let pnp_query = "SELECT Name, DeviceID, Manufacturer, PNPClass, Status FROM Win32_PnPEntity WHERE DeviceID LIKE 'USB%' AND PNPClass != 'USB'";
    if let Ok(pnp_devices) = wmi_conn.raw_query::<Win32PnPEntity>(pnp_query) {
        for device in pnp_devices {
            if let Some(name) = device.name {
                // Skip controllers and hubs we already have
                if name.to_lowercase().contains("controller") || name.to_lowercase().contains("hub")
                {
                    continue;
                }

                let usb_version = detect_usb_version(&name);

                devices.push(UsbDeviceInfo {
                    name,
                    device_id: device.device_id.clone(),
                    vendor: device.manufacturer,
                    product_id: None,
                    vendor_id: extract_vendor_id(&device.device_id),
                    usb_version,
                    device_class: device.pnp_class,
                    status: device.status,
                    hub_port: None,
                });
            }
        }
    }

    devices
}

/// Detect USB version from device name
fn detect_usb_version(name: &str) -> UsbVersion {
    let name_lower = name.to_lowercase();

    if name_lower.contains("usb4") || name_lower.contains("usb 4") {
        UsbVersion::Usb4
    } else if name_lower.contains("3.2") || name_lower.contains("superspeedplus") {
        UsbVersion::Usb3_2
    } else if name_lower.contains("3.1") {
        UsbVersion::Usb3_1
    } else if name_lower.contains("3.0")
        || name_lower.contains("xhci")
        || name_lower.contains("superspeed")
    {
        UsbVersion::Usb3_0
    } else if name_lower.contains("2.0")
        || name_lower.contains("ehci")
        || name_lower.contains("enhanced")
    {
        UsbVersion::Usb2_0
    } else if name_lower.contains("1.1")
        || name_lower.contains("uhci")
        || name_lower.contains("ohci")
    {
        UsbVersion::Usb1_1
    } else {
        UsbVersion::Unknown
    }
}

/// Extract vendor ID from PNP device ID
fn extract_vendor_id(device_id: &Option<String>) -> Option<String> {
    device_id.as_ref().and_then(|id| {
        // Format: USB\VID_XXXX&PID_YYYY\...
        id.split('\\').nth(1).and_then(|part| {
            part.split('&')
                .find(|p| p.starts_with("VID_"))
                .map(|v| v.trim_start_matches("VID_").to_string())
        })
    })
}

/// Get display outputs from WMI
fn get_display_outputs(wmi_conn: &WMIConnection) -> Vec<DisplayOutputInfo> {
    let mut outputs = Vec::new();

    // Query video controllers
    let video_query = "SELECT Name, AdapterCompatibility, VideoProcessor, CurrentHorizontalResolution, CurrentVerticalResolution, CurrentRefreshRate, Status FROM Win32_VideoController";
    if let Ok(controllers) = wmi_conn.raw_query::<Win32VideoController>(video_query) {
        for controller in controllers {
            let name = controller
                .name
                .unwrap_or_else(|| "Unknown Display Adapter".to_string());

            // Infer output type from adapter name
            let output_type = detect_display_output_type(&name);

            let resolution = match (
                controller.current_horizontal_resolution,
                controller.current_vertical_resolution,
            ) {
                (Some(w), Some(h)) => Some(format!("{}x{}", w, h)),
                _ => None,
            };

            outputs.push(DisplayOutputInfo {
                name: name.clone(),
                output_type,
                connected: controller.status.as_deref() == Some("OK"),
                resolution,
                refresh_rate: controller.current_refresh_rate,
                adapter: controller.adapter_compatibility.or(Some(name)),
            });
        }
    }

    // Query monitors for additional info
    let monitor_query = "SELECT Name, MonitorManufacturer, MonitorType, ScreenWidth, ScreenHeight FROM Win32_DesktopMonitor";
    if let Ok(monitors) = wmi_conn.raw_query::<Win32DesktopMonitor>(monitor_query) {
        for monitor in monitors {
            let name = monitor.name.unwrap_or_else(|| "Monitor".to_string());
            let output_type = detect_display_output_type(&name);

            let resolution = match (monitor.screen_width, monitor.screen_height) {
                (Some(w), Some(h)) if w > 0 && h > 0 => Some(format!("{}x{}", w, h)),
                _ => None,
            };

            // Only add if we don't already have this from video controllers
            if !outputs
                .iter()
                .any(|o| o.resolution == resolution && resolution.is_some())
            {
                outputs.push(DisplayOutputInfo {
                    name,
                    output_type,
                    connected: true,
                    resolution,
                    refresh_rate: None,
                    adapter: monitor.monitor_manufacturer,
                });
            }
        }
    }

    outputs
}

/// Detect display output type from name
fn detect_display_output_type(name: &str) -> DisplayOutputType {
    let name_lower = name.to_lowercase();

    if name_lower.contains("hdmi") {
        DisplayOutputType::Hdmi
    } else if name_lower.contains("displayport") || name_lower.contains("dp") {
        DisplayOutputType::DisplayPort
    } else if name_lower.contains("dvi") {
        DisplayOutputType::Dvi
    } else if name_lower.contains("vga") {
        DisplayOutputType::Vga
    } else if name_lower.contains("thunderbolt") {
        DisplayOutputType::Thunderbolt
    } else if name_lower.contains("usb-c") || name_lower.contains("type-c") {
        DisplayOutputType::UsbC
    } else if name_lower.contains("internal")
        || name_lower.contains("laptop")
        || name_lower.contains("built-in")
    {
        DisplayOutputType::Internal
    } else {
        DisplayOutputType::Unknown
    }
}

/// Get audio devices from WMI
fn get_audio_devices(wmi_conn: &WMIConnection) -> Vec<AudioDeviceInfo> {
    let mut devices = Vec::new();

    let audio_query =
        "SELECT Name, Manufacturer, Status, DeviceID, PNPDeviceID FROM Win32_SoundDevice";
    if let Ok(sound_devices) = wmi_conn.raw_query::<Win32SoundDevice>(audio_query) {
        for device in sound_devices {
            let name = device.name.unwrap_or_else(|| "Audio Device".to_string());

            // Determine device type from name
            let device_type = detect_audio_device_type(&name);

            devices.push(AudioDeviceInfo {
                name,
                device_type,
                manufacturer: device.manufacturer,
                status: device.status,
                is_default: false, // Would need additional API to determine
            });
        }
    }

    devices
}

/// Detect audio device type from name
fn detect_audio_device_type(name: &str) -> AudioDeviceType {
    let name_lower = name.to_lowercase();

    if name_lower.contains("microphone")
        || name_lower.contains("mic")
        || name_lower.contains("input")
    {
        AudioDeviceType::Input
    } else if name_lower.contains("headset") || name_lower.contains("headphone") {
        AudioDeviceType::OutputInput
    } else if name_lower.contains("speaker")
        || name_lower.contains("output")
        || name_lower.contains("audio")
    {
        AudioDeviceType::Output
    } else {
        AudioDeviceType::Unknown
    }
}

/// Get network ports from WMI
fn get_network_ports(wmi_conn: &WMIConnection) -> Vec<NetworkPortInfo> {
    let mut ports = Vec::new();

    // Query network adapters
    let net_query = "SELECT Description, MACAddress, IPAddress, IPEnabled FROM Win32_NetworkAdapterConfiguration WHERE IPEnabled = True";
    if let Ok(adapters) = wmi_conn.raw_query::<Win32NetworkAdapterConfig>(net_query) {
        for adapter in adapters {
            let name = adapter
                .description
                .unwrap_or_else(|| "Network Adapter".to_string());

            let port_type = detect_network_port_type(&name);
            let speed = detect_network_speed(&name);

            ports.push(NetworkPortInfo {
                name,
                port_type,
                speed,
                mac_address: adapter.mac_address,
                connected: adapter.ip_enabled.unwrap_or(false),
            });
        }
    }

    ports
}

/// Detect network port type from name
fn detect_network_port_type(name: &str) -> NetworkPortType {
    let name_lower = name.to_lowercase();

    if name_lower.contains("wifi")
        || name_lower.contains("wi-fi")
        || name_lower.contains("wireless")
        || name_lower.contains("802.11")
    {
        NetworkPortType::WiFi
    } else if name_lower.contains("bluetooth") {
        NetworkPortType::Bluetooth
    } else if name_lower.contains("thunderbolt") {
        NetworkPortType::Thunderbolt
    } else if name_lower.contains("ethernet")
        || name_lower.contains("realtek")
        || name_lower.contains("intel")
        || name_lower.contains("gigabit")
    {
        NetworkPortType::Ethernet
    } else {
        NetworkPortType::Other
    }
}

/// Detect network speed from adapter name
fn detect_network_speed(name: &str) -> Option<String> {
    let name_lower = name.to_lowercase();

    if name_lower.contains("10gbe") || name_lower.contains("10 gigabit") {
        Some("10 Gbps".to_string())
    } else if name_lower.contains("5gbe") || name_lower.contains("5 gigabit") {
        Some("5 Gbps".to_string())
    } else if name_lower.contains("2.5gbe") || name_lower.contains("2.5 gigabit") {
        Some("2.5 Gbps".to_string())
    } else if name_lower.contains("gigabit")
        || name_lower.contains("gbe")
        || name_lower.contains("1000")
    {
        Some("1 Gbps".to_string())
    } else if name_lower.contains("fast ethernet") || name_lower.contains("100") {
        Some("100 Mbps".to_string())
    } else if name_lower.contains("wifi 6e") || name_lower.contains("ax") {
        Some("Wi-Fi 6E".to_string())
    } else if name_lower.contains("wifi 6") || name_lower.contains("802.11ax") {
        Some("Wi-Fi 6".to_string())
    } else if name_lower.contains("wifi 5") || name_lower.contains("802.11ac") {
        Some("Wi-Fi 5".to_string())
    } else {
        None
    }
}
