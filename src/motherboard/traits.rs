// Unified traits for motherboard and system monitoring

use serde::{Deserialize, Serialize};
use std::fmt;

/// Trait for motherboard sensor devices
pub trait MotherboardDevice: Send + Sync {
    /// Get the device/chip name (e.g., "nct6798", "it8792e", "coretemp")
    fn name(&self) -> &str;

    /// Get the device path (platform-specific)
    fn device_path(&self) -> Option<String>;

    /// Get all temperature sensors
    fn temperature_sensors(&self) -> Result<Vec<TemperatureSensor>, Error>;

    /// Get all voltage rails
    fn voltage_rails(&self) -> Result<Vec<VoltageRail>, Error>;

    /// Get all fan information
    fn fans(&self) -> Result<Vec<FanInfo>, Error>;

    /// Set fan speed (if supported)
    fn set_fan_speed(&self, fan_index: usize, speed: FanControl) -> Result<(), Error> {
        let _ = (fan_index, speed);
        Err(Error::NotSupported("Fan control not supported".into()))
    }
}

/// Temperature sensor reading
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemperatureSensor {
    /// Sensor label (e.g., "CPU", "Chipset", "VRM", "Ambient")
    pub label: String,
    /// Current temperature in Celsius, or `None` where the sensor exists but
    /// could not be read.
    ///
    /// **Optional because the thresholds beside it always were.** A bare `f32`
    /// left a reader with nowhere to put "this board exposes a sensor I cannot
    /// sample", so one backend pushed a sensor reading 0.0 °C with a comment
    /// saying real values need an API it does not call. A die at 0 °C is not a
    /// plausible reading; it is the absence of one.
    pub temperature: Option<f32>,
    /// Maximum safe temperature (optional)
    pub max: Option<f32>,
    /// Critical temperature threshold (optional)
    pub critical: Option<f32>,
    /// Sensor type
    pub sensor_type: SensorType,
}

/// Voltage rail reading
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoltageRail {
    /// Rail label (e.g., "VCore", "+12V", "+5V", "VDIMM")
    pub label: String,
    /// Current voltage in volts, or `None` where the rail is exposed but not
    /// readable. See [`TemperatureSensor::temperature`].
    pub voltage: Option<f32>,
    /// Minimum acceptable voltage (optional)
    pub min: Option<f32>,
    /// Maximum acceptable voltage (optional)
    pub max: Option<f32>,
}

/// Fan information and control
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FanInfo {
    /// Fan label (e.g., "CPU Fan", "Case Fan 1", "Chipset Fan")
    pub label: String,
    /// Current speed in RPM (None if stopped or not measurable)
    pub rpm: Option<u32>,
    /// Current PWM duty cycle (0-255, if available)
    pub pwm: Option<u8>,
    /// Minimum RPM threshold (optional)
    pub min_rpm: Option<u32>,
    /// Maximum RPM capability (optional)
    pub max_rpm: Option<u32>,
    /// Whether fan control is supported
    pub controllable: bool,
}

/// Fan control mode
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum FanControl {
    /// Manual PWM value (0-255)
    Manual(u8),
    /// Automatic control by hardware/firmware
    Automatic,
}

/// Sensor type classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SensorType {
    /// CPU package/core temperature
    Cpu,
    /// GPU temperature
    Gpu,
    /// Chipset temperature
    Chipset,
    /// VRM (Voltage Regulator Module) temperature
    Vrm,
    /// Ambient/system temperature
    Ambient,
    /// M.2/NVMe slot temperature
    M2Slot,
    /// PCH (Platform Controller Hub) temperature
    Pch,
    /// Storage device temperature (HDD/SSD/NVMe)
    Storage,
    /// Other/unknown sensor type
    Other,
}

/// Generic sensor reading
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorReading {
    /// Sensor label.
    pub label: String,
    /// The reading, or `None` where the sensor is present and unreadable.
    pub value: Option<f32>,
    /// Unit the value is expressed in.
    pub unit: String,
}

/// System information (OS, hardware, BIOS)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    // Operating System
    pub os_name: String,
    pub os_version: String,
    pub kernel_version: Option<String>,
    pub architecture: String, // x86_64, aarch64, etc.
    pub hostname: Option<String>,

    // BIOS/UEFI
    pub bios: BiosInfo,

    // Hardware
    pub manufacturer: Option<String>, // Dell, ASUS, etc.
    pub product_name: Option<String>, // Model name
    pub serial_number: Option<String>,
    pub uuid: Option<String>,

    // Motherboard
    pub board_vendor: Option<String>,
    pub board_name: Option<String>,
    pub board_version: Option<String>,

    // CPU
    pub cpu_name: Option<String>,
    pub cpu_cores: Option<u32>,
    pub cpu_threads: Option<u32>,
}

/// BIOS/UEFI information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiosInfo {
    pub vendor: Option<String>,       // AMI, Award, Phoenix, etc.
    pub version: Option<String>,      // BIOS version string
    pub release_date: Option<String>, // Release date
    pub revision: Option<String>,     // BIOS revision
    pub firmware_type: FirmwareType,  // Legacy BIOS or UEFI
    pub secure_boot: Option<bool>,    // Secure Boot status (UEFI)
}

/// Firmware type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FirmwareType {
    /// Legacy BIOS
    Bios,
    /// UEFI firmware
    Uefi,
    /// Unknown/undetermined
    Unknown,
}

/// PCIe device information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PcieDeviceInfo {
    pub name: String,
    pub device_id: Option<String>,
    pub vendor: Option<String>,
    pub pcie_version: Option<String>, // PCIe 3.0, 4.0, 5.0
    pub link_width: Option<u8>,       // x1, x4, x8, x16
    pub link_speed: Option<String>,   // 2.5 GT/s, 5 GT/s, 8 GT/s, etc.
    pub slot: Option<String>,
    pub device_class: Option<String>, // VGA, Network, Storage, etc.
}

/// SATA device information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SataDeviceInfo {
    pub name: String,
    pub model: Option<String>,
    pub serial: Option<String>,
    pub firmware: Option<String>,
    pub capacity_gb: Option<f64>,
    pub interface_speed: Option<String>, // SATA I, II, III
    pub port: Option<u8>,
    pub temperature: Option<f32>, // Celsius
    pub media_type: SataMediaType,
}

/// SATA media type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SataMediaType {
    Hdd,
    Ssd,
    Unknown,
}

/// System temperature summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemTemperatures {
    pub cpu: Option<f32>,
    pub gpu: Option<f32>,
    pub motherboard: Option<f32>,
    pub storage: Vec<(String, f32)>, // Device name, temperature
    pub network: Vec<(String, f32)>, // Device name, temperature
}

/// USB device information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsbDeviceInfo {
    pub name: String,
    pub device_id: Option<String>,
    pub vendor: Option<String>,
    pub product_id: Option<String>,
    pub vendor_id: Option<String>,
    pub usb_version: UsbVersion,
    pub device_class: Option<String>,
    pub status: Option<String>,
    pub hub_port: Option<u8>,
}

/// USB version classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UsbVersion {
    Usb1_1,
    Usb2_0,
    Usb3_0,
    Usb3_1,
    Usb3_2,
    Usb4,
    Unknown,
}

impl std::fmt::Display for UsbVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UsbVersion::Usb1_1 => write!(f, "USB 1.1"),
            UsbVersion::Usb2_0 => write!(f, "USB 2.0"),
            UsbVersion::Usb3_0 => write!(f, "USB 3.0"),
            UsbVersion::Usb3_1 => write!(f, "USB 3.1"),
            UsbVersion::Usb3_2 => write!(f, "USB 3.2"),
            UsbVersion::Usb4 => write!(f, "USB4"),
            UsbVersion::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Display output information (HDMI, DP, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayOutputInfo {
    pub name: String,
    pub output_type: DisplayOutputType,
    pub connected: bool,
    pub resolution: Option<String>,
    pub refresh_rate: Option<u32>,
    pub adapter: Option<String>,
}

/// Display output type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DisplayOutputType {
    Hdmi,
    DisplayPort,
    Dvi,
    Vga,
    Thunderbolt,
    UsbC,
    Internal,
    Unknown,
}

impl std::fmt::Display for DisplayOutputType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DisplayOutputType::Hdmi => write!(f, "HDMI"),
            DisplayOutputType::DisplayPort => write!(f, "DisplayPort"),
            DisplayOutputType::Dvi => write!(f, "DVI"),
            DisplayOutputType::Vga => write!(f, "VGA"),
            DisplayOutputType::Thunderbolt => write!(f, "Thunderbolt"),
            DisplayOutputType::UsbC => write!(f, "USB-C"),
            DisplayOutputType::Internal => write!(f, "Internal"),
            DisplayOutputType::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Audio device information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioDeviceInfo {
    pub name: String,
    pub device_type: AudioDeviceType,
    pub manufacturer: Option<String>,
    pub status: Option<String>,
    pub is_default: bool,
}

/// Audio device type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AudioDeviceType {
    Output,      // Speakers, headphones
    Input,       // Microphone
    OutputInput, // Combined (headset)
    Unknown,
}

impl std::fmt::Display for AudioDeviceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AudioDeviceType::Output => write!(f, "Output"),
            AudioDeviceType::Input => write!(f, "Input"),
            AudioDeviceType::OutputInput => write!(f, "Input/Output"),
            AudioDeviceType::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Bluetooth device information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BluetoothDeviceInfo {
    pub name: String,
    pub address: Option<String>,
    pub device_type: Option<String>,
    pub connected: bool,
    pub paired: bool,
}

/// Network port information (Ethernet, WiFi adapters)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkPortInfo {
    pub name: String,
    pub port_type: NetworkPortType,
    pub speed: Option<String>,
    pub mac_address: Option<String>,
    pub connected: bool,
}

/// Network port type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NetworkPortType {
    Ethernet,
    WiFi,
    Bluetooth,
    Thunderbolt,
    Other,
}

impl std::fmt::Display for NetworkPortType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetworkPortType::Ethernet => write!(f, "Ethernet"),
            NetworkPortType::WiFi => write!(f, "Wi-Fi"),
            NetworkPortType::Bluetooth => write!(f, "Bluetooth"),
            NetworkPortType::Thunderbolt => write!(f, "Thunderbolt"),
            NetworkPortType::Other => write!(f, "Other"),
        }
    }
}

/// All peripherals summary
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PeripheralsInfo {
    pub usb_devices: Vec<UsbDeviceInfo>,
    pub display_outputs: Vec<DisplayOutputInfo>,
    pub audio_devices: Vec<AudioDeviceInfo>,
    pub bluetooth_devices: Vec<BluetoothDeviceInfo>,
    pub network_ports: Vec<NetworkPortInfo>,
}

/// Driver/module information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriverInfo {
    pub name: String,
    pub version: String,
    pub driver_type: DriverType,
    pub description: Option<String>,
    pub vendor: Option<String>,
    pub date: Option<String>,
}

/// Driver type classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DriverType {
    /// GPU driver (NVIDIA, AMD, Intel)
    Gpu,
    /// Chipset driver
    Chipset,
    /// Storage controller (SATA, NVMe, RAID)
    Storage,
    /// Network adapter
    Network,
    /// Audio device
    Audio,
    /// USB controller
    Usb,
    /// Other/unknown driver type
    Other,
}

impl fmt::Display for DriverType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DriverType::Gpu => write!(f, "GPU"),
            DriverType::Chipset => write!(f, "Chipset"),
            DriverType::Storage => write!(f, "Storage"),
            DriverType::Network => write!(f, "Network"),
            DriverType::Audio => write!(f, "Audio"),
            DriverType::Usb => write!(f, "USB"),
            DriverType::Other => write!(f, "Other"),
        }
    }
}

/// Error types for motherboard monitoring
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Operation not supported: {0}")]
    NotSupported(String),

    #[error("No sensors found")]
    NoSensorsFound,

    #[error("Initialization failed: {0}")]
    InitializationFailed(String),

    #[error("Query failed: {0}")]
    QueryFailed(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Fan control error: {0}")]
    FanControlError(String),

    #[error("Unknown error: {0}")]
    Unknown(String),
}
