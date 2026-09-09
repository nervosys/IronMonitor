//! Environmental sensor monitoring — accelerometer, gyroscope, light, proximity.
//!
//! # Platform Support
//!
//! - **Linux**: Reads IIO (Industrial I/O) subsystem at `/sys/bus/iio/devices/`
//! - **Windows**: Uses WMI (`Win32_Sensor`) and Windows Sensor API
//! - **macOS**: Uses `IOKit` sensor interfaces
//!
//! # Examples
//!
//! ```no_run
//! use ironmonlib::sensors::SensorMonitor;
//!
//! let monitor = SensorMonitor::new().unwrap();
//! for sensor in monitor.sensors() {
//!     println!("{}: {:?} = {:?}", sensor.name, sensor.sensor_type, sensor.values);
//! }
//! ```

use crate::error::IronError;
use serde::{Deserialize, Serialize};

/// Type of environmental sensor
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SensorType {
    /// 3-axis accelerometer (m/s²)
    Accelerometer,
    /// 3-axis gyroscope (rad/s)
    Gyroscope,
    /// 3-axis magnetometer (µT)
    Magnetometer,
    /// Ambient light sensor (lux)
    AmbientLight,
    /// Proximity sensor (cm or boolean)
    Proximity,
    /// Barometric pressure (hPa / mbar)
    Pressure,
    /// Humidity sensor (% RH)
    Humidity,
    /// Temperature sensor (°C) — environmental, not CPU
    Temperature,
    /// Orientation / rotation vector
    Orientation,
    /// Gravity sensor
    Gravity,
    /// Step counter / pedometer
    StepCounter,
    /// Fingerprint sensor
    Fingerprint,
    /// UV index sensor
    UVIndex,
    /// Color sensor (RGB)
    ColorSensor,
    /// Current sensor (A)
    Current,
    /// Voltage sensor (V)
    Voltage,
    /// Other / unknown
    Other(String),
}

/// Sensor reading value
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorValue {
    /// Axis or channel name (e.g., "x", "y", "z", "lux", "raw")
    pub channel: String,
    /// Raw value
    pub raw: f64,
    /// Scale factor
    pub scale: f64,
    /// Offset
    pub offset: f64,
    /// Unit (e.g., "m/s²", "lux", "hPa")
    pub unit: String,
}

impl SensorValue {
    /// Compute scaled value: (raw + offset) * scale
    pub fn scaled_value(&self) -> f64 {
        (self.raw + self.offset) * self.scale
    }
}

/// Information about a single sensor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorInfo {
    /// Sensor name
    pub name: String,
    /// Sensor type
    pub sensor_type: SensorType,
    /// Device path or identifier
    pub device: String,
    /// Current values per channel
    pub values: Vec<SensorValue>,
    /// Sampling frequency in Hz (if available)
    pub sampling_frequency_hz: Option<f64>,
    /// Whether the sensor is currently active
    pub active: bool,
    /// Vendor/manufacturer
    pub vendor: String,
}

/// Monitor for environmental sensors
pub struct SensorMonitor {
    items: Vec<SensorInfo>,
    /// Why `items` is empty, when the reason is something other than "there are
    /// none". See [`SensorMonitor::note`].
    last_note: Option<String>,
}

impl SensorMonitor {
    pub fn new() -> Result<Self, IronError> {
        let mut monitor = Self {
            items: Vec::new(),
            last_note: None,
        };
        monitor.refresh()?;
        Ok(monitor)
    }

    /// Re-enumerate the platform's sensors.
    ///
    /// Returns `Err` when the enumeration failed, and `Ok` with an empty list
    /// only when it succeeded and found nothing -- which the resolver publishes
    /// as `board.sensor.<none>`, a claim that this board has no sensors.
    /// See [`crate::core::command`].
    pub fn refresh(&mut self) -> Result<(), IronError> {
        self.items.clear();
        self.last_note = None;

        #[cfg(target_os = "linux")]
        self.refresh_linux()?;

        #[cfg(target_os = "windows")]
        self.refresh_windows()?;

        #[cfg(target_os = "macos")]
        self.refresh_macos()?;

        Ok(())
    }

    pub fn sensors(&self) -> &[SensorInfo] {
        &self.items
    }

    /// Why the sensor list is empty, when it is.
    ///
    /// This used to be the only place a failure was recorded, and it was fed by
    /// a probe -- a `powershell -Command "exit 0"` spawned purely to see whether
    /// spawning worked -- while the real query one line below still swallowed a
    /// missing helper, a non-zero exit and unparseable output alike. Those now
    /// arrive as an `Err` from [`Self::refresh`], which is where the reason
    /// belongs.
    ///
    /// What remains for this accessor is [`Default`], which calls
    /// `new().unwrap_or(..)` and so has nowhere to put an error: it records one
    /// here instead.
    ///
    /// `None` alongside an empty list means the enumeration ran and found
    /// nothing — a desktop with no ambient-light or accelerometer sensors, which
    /// is the common case and a true reading.
    pub fn note(&self) -> Option<&str> {
        self.last_note.as_deref()
    }

    /// Get sensors by type.
    pub fn sensors_by_type(&self, sensor_type: &SensorType) -> Vec<&SensorInfo> {
        self.items
            .iter()
            .filter(|s| &s.sensor_type == sensor_type)
            .collect()
    }

    /// Check if accelerometer is present.
    pub fn has_accelerometer(&self) -> bool {
        self.items
            .iter()
            .any(|s| s.sensor_type == SensorType::Accelerometer)
    }

    /// Check if ambient light sensor is present.
    pub fn has_light_sensor(&self) -> bool {
        self.items
            .iter()
            .any(|s| s.sensor_type == SensorType::AmbientLight)
    }

    /// Get current ambient light level in lux.
    pub fn ambient_light_lux(&self) -> Option<f64> {
        self.items
            .iter()
            .find(|s| s.sensor_type == SensorType::AmbientLight)
            .and_then(|s| s.values.first())
            .map(|v| v.scaled_value())
    }

    #[cfg(target_os = "linux")]
    fn refresh_linux(&mut self) -> Result<(), IronError> {
        let iio_base = std::path::Path::new("/sys/bus/iio/devices");
        if !iio_base.exists() {
            // A real answer: this kernel exposes no IIO subsystem.
            return Ok(());
        }

        let entries = std::fs::read_dir(iio_base)
            .map_err(|e| IronError::System(format!("cannot read {iio_base:?}: {e}")))?;
        for entry in entries.flatten() {
            let base = entry.path();
            let dev_name = entry.file_name().to_string_lossy().to_string();

            let name = Self::read_trimmed(&base.join("name"));
            if name.is_empty() {
                continue;
            }

            // Determine type by scanning available channels
            let sensor_type = Self::detect_iio_type(&base);
            let mut values = Vec::new();

            // Read channels based on type
            match &sensor_type {
                SensorType::Accelerometer => {
                    for axis in &["x", "y", "z"] {
                        if let Some(val) = Self::read_iio_channel(&base, "accel", axis) {
                            values.push(val);
                        }
                    }
                }
                SensorType::Gyroscope => {
                    for axis in &["x", "y", "z"] {
                        if let Some(val) = Self::read_iio_channel(&base, "anglvel", axis) {
                            values.push(val);
                        }
                    }
                }
                SensorType::Magnetometer => {
                    for axis in &["x", "y", "z"] {
                        if let Some(val) = Self::read_iio_channel(&base, "magn", axis) {
                            values.push(val);
                        }
                    }
                }
                SensorType::AmbientLight => {
                    if let Some(val) = Self::read_iio_single(&base, "in_illuminance_raw", "lux") {
                        values.push(val);
                    }
                }
                SensorType::Proximity => {
                    if let Some(val) = Self::read_iio_single(&base, "in_proximity_raw", "cm") {
                        values.push(val);
                    }
                }
                SensorType::Pressure => {
                    if let Some(val) = Self::read_iio_single(&base, "in_pressure_raw", "hPa") {
                        values.push(val);
                    }
                }
                SensorType::Humidity => {
                    if let Some(val) =
                        Self::read_iio_single(&base, "in_humidityrelative_raw", "%RH")
                    {
                        values.push(val);
                    }
                }
                SensorType::Temperature => {
                    if let Some(val) = Self::read_iio_single(&base, "in_temp_raw", "°C") {
                        values.push(val);
                    }
                }
                _ => {}
            }

            let freq = Self::read_trimmed(&base.join("sampling_frequency"))
                .parse::<f64>()
                .ok();

            self.items.push(SensorInfo {
                name,
                sensor_type,
                device: dev_name,
                values,
                sampling_frequency_hz: freq,
                active: true,
                vendor: String::new(),
            });
        }

        // Also check hwmon for voltage/current sensors
        let hwmon_base = std::path::Path::new("/sys/class/hwmon");
        if hwmon_base.exists() {
            if let Ok(entries) = std::fs::read_dir(hwmon_base) {
                for entry in entries.flatten() {
                    let base = entry.path();
                    let hwmon_name = Self::read_trimmed(&base.join("name"));

                    // Check for voltage inputs
                    for i in 0..16 {
                        let input_path = base.join(format!("in{}_input", i));
                        if input_path.exists() {
                            let raw: f64 = Self::read_trimmed(&input_path).parse().unwrap_or(0.0);
                            let label_path = base.join(format!("in{}_label", i));
                            let label = Self::read_trimmed(&label_path);
                            let channel = if label.is_empty() {
                                format!("in{}", i)
                            } else {
                                label
                            };

                            // hwmon voltage is in millivolts
                            self.items.push(SensorInfo {
                                name: format!("{} voltage {}", hwmon_name, channel),
                                sensor_type: SensorType::Voltage,
                                device: entry.file_name().to_string_lossy().to_string(),
                                values: vec![SensorValue {
                                    channel,
                                    raw: raw / 1000.0,
                                    scale: 1.0,
                                    offset: 0.0,
                                    unit: "V".to_string(),
                                }],
                                sampling_frequency_hz: None,
                                active: true,
                                vendor: String::new(),
                            });
                        }
                    }

                    // Current sensors
                    for i in 0..8 {
                        let input_path = base.join(format!("curr{}_input", i));
                        if input_path.exists() {
                            let raw: f64 = Self::read_trimmed(&input_path).parse().unwrap_or(0.0);
                            self.items.push(SensorInfo {
                                name: format!("{} current {}", hwmon_name, i),
                                sensor_type: SensorType::Current,
                                device: entry.file_name().to_string_lossy().to_string(),
                                values: vec![SensorValue {
                                    channel: format!("curr{}", i),
                                    raw: raw / 1000.0,
                                    scale: 1.0,
                                    offset: 0.0,
                                    unit: "A".to_string(),
                                }],
                                sampling_frequency_hz: None,
                                active: true,
                                vendor: String::new(),
                            });
                        }
                    }
                }
            }
        }
        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn detect_iio_type(base: &std::path::Path) -> SensorType {
        if let Ok(entries) = std::fs::read_dir(base) {
            let filenames: Vec<String> = entries
                .flatten()
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect();
            let joined = filenames.join(" ");
            if joined.contains("in_accel") {
                return SensorType::Accelerometer;
            }
            if joined.contains("in_anglvel") {
                return SensorType::Gyroscope;
            }
            if joined.contains("in_magn") {
                return SensorType::Magnetometer;
            }
            if joined.contains("in_illuminance") {
                return SensorType::AmbientLight;
            }
            if joined.contains("in_proximity") {
                return SensorType::Proximity;
            }
            if joined.contains("in_pressure") {
                return SensorType::Pressure;
            }
            if joined.contains("in_humidityrelative") {
                return SensorType::Humidity;
            }
            if joined.contains("in_temp") {
                return SensorType::Temperature;
            }
        }
        SensorType::Other("unknown".into())
    }

    #[cfg(target_os = "linux")]
    fn read_iio_channel(base: &std::path::Path, prefix: &str, axis: &str) -> Option<SensorValue> {
        let raw_path = base.join(format!("in_{}_{}_raw", prefix, axis));
        let raw: f64 = std::fs::read_to_string(&raw_path)
            .ok()?
            .trim()
            .parse()
            .ok()?;
        let scale: f64 =
            std::fs::read_to_string(base.join(format!("in_{}_{}_scale", prefix, axis)))
                .or_else(|_| std::fs::read_to_string(base.join(format!("in_{}_scale", prefix))))
                .unwrap_or_else(|_| "1.0".to_string())
                .trim()
                .parse()
                .unwrap_or(1.0);
        let offset: f64 =
            std::fs::read_to_string(base.join(format!("in_{}_{}_offset", prefix, axis)))
                .or_else(|_| std::fs::read_to_string(base.join(format!("in_{}_offset", prefix))))
                .unwrap_or_else(|_| "0.0".to_string())
                .trim()
                .parse()
                .unwrap_or(0.0);

        let unit = match prefix {
            "accel" => "m/s²",
            "anglvel" => "rad/s",
            "magn" => "µT",
            _ => "",
        };

        Some(SensorValue {
            channel: axis.to_string(),
            raw,
            scale,
            offset,
            unit: unit.to_string(),
        })
    }

    #[cfg(target_os = "linux")]
    fn read_iio_single(base: &std::path::Path, filename: &str, unit: &str) -> Option<SensorValue> {
        let raw: f64 = std::fs::read_to_string(base.join(filename))
            .ok()?
            .trim()
            .parse()
            .ok()?;
        let scale_file = filename.replace("_raw", "_scale");
        let scale: f64 = std::fs::read_to_string(base.join(&scale_file))
            .unwrap_or_else(|_| "1.0".to_string())
            .trim()
            .parse()
            .unwrap_or(1.0);
        let offset_file = filename.replace("_raw", "_offset");
        let offset: f64 = std::fs::read_to_string(base.join(&offset_file))
            .unwrap_or_else(|_| "0.0".to_string())
            .trim()
            .parse()
            .unwrap_or(0.0);
        Some(SensorValue {
            channel: filename.replace("in_", "").replace("_raw", ""),
            raw,
            scale,
            offset,
            unit: unit.to_string(),
        })
    }

    #[cfg(target_os = "linux")]
    fn read_trimmed(path: &std::path::Path) -> String {
        std::fs::read_to_string(path)
            .unwrap_or_default()
            .trim()
            .to_string()
    }

    #[cfg(target_os = "windows")]
    fn refresh_windows(&mut self) -> Result<(), IronError> {
        // This function already knew that "no sensors" and "could not ask" are
        // different: it spawned `powershell -Command "exit 0"` first, purely so
        // it could record a note when that spawn failed. The distinction was
        // drawn in the probe and thrown away by the real query one line later,
        // which swallowed a spawn failure, a non-zero exit and unparseable
        // output alike. The probe is gone; the queries report for themselves.
        // `MSFT_Sensor` does not exist in `root/standardcimv2` on
        // Windows 11 -- `Get-CimClass` there answers "Invalid class" and
        // PowerShell exits 1 even under `-ErrorAction SilentlyContinue`.
        // So this query has never returned a sensor on such a host, and
        // swallowing its failure is precisely why nobody noticed. Asking
        // for the class first keeps the two answers apart: a platform
        // that does not define it prints nothing and exits 0, while a
        // query that genuinely fails still reports.
        const SENSOR_API: &str = r#"if (Get-CimClass -Namespace root/standardcimv2 -ClassName MSFT_Sensor -ErrorAction SilentlyContinue) { Get-CimInstance -Namespace root/standardcimv2 -ClassName MSFT_Sensor | Select-Object SensorType, FriendlyName, CurrentState | ConvertTo-Json -Compress }"#;
        const HID_SENSORS: &str = r#"Get-CimInstance Win32_PnPEntity | Where-Object { $_.PNPClass -eq 'Sensor' } | Select-Object Name, Manufacturer, Status | ConvertTo-Json -Compress"#;

        if let Some(val) = crate::core::command::capture_json(
            "powershell",
            &["-NoProfile", "-Command", SENSOR_API],
        )? {
            for item in &crate::core::command::json_items(&val) {
                // A sensor whose name the platform did not give is skipped
                // rather than named "Unknown". A literal "Unknown" in a name
                // field reads as the sensor's actual name, which is the exact
                // mistake `unknown_is_never_dressed_as_a_measurement` exists to
                // catch one layer down.
                let Some(name) = item["FriendlyName"].as_str().map(str::to_string) else {
                    continue;
                };
                let type_str = item["SensorType"].as_str().unwrap_or("");
                let sensor_type = Self::parse_sensor_type_win(type_str, &name);

                self.items.push(SensorInfo {
                    name,
                    sensor_type,
                    device: String::new(),
                    values: Vec::new(),
                    sampling_frequency_hz: None,
                    active: item["CurrentState"].as_str() == Some("Ready"),
                    vendor: String::new(),
                });
            }
        }

        // Also check for HID sensors (ambient light, accelerometer) via PnP.
        // A failure here is propagated rather than keeping whatever the first
        // query found: half an enumeration is not an enumeration, and the
        // caller has no way to tell the difference.
        if let Some(val) = crate::core::command::capture_json(
            "powershell",
            &["-NoProfile", "-Command", HID_SENSORS],
        )? {
            for item in &crate::core::command::json_items(&val) {
                let name = item["Name"].as_str().unwrap_or("").to_string();
                let sensor_type = Self::infer_sensor_type(&name);
                self.items.push(SensorInfo {
                    name,
                    sensor_type,
                    device: String::new(),
                    values: Vec::new(),
                    sampling_frequency_hz: None,
                    active: item["Status"].as_str() == Some("OK"),
                    vendor: item["Manufacturer"].as_str().unwrap_or("").to_string(),
                });
            }
        }

        Ok(())
    }

    #[cfg(target_os = "windows")]
    fn parse_sensor_type_win(type_str: &str, name: &str) -> SensorType {
        let combined = format!("{} {}", type_str, name).to_lowercase();
        Self::infer_sensor_type(&combined)
    }

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    fn infer_sensor_type(name: &str) -> SensorType {
        let lower = name.to_lowercase();
        if lower.contains("accel") {
            SensorType::Accelerometer
        } else if lower.contains("gyro") {
            SensorType::Gyroscope
        } else if lower.contains("magn") || lower.contains("compass") {
            SensorType::Magnetometer
        } else if lower.contains("light") || lower.contains("illum") || lower.contains("als") {
            SensorType::AmbientLight
        } else if lower.contains("prox") {
            SensorType::Proximity
        } else if lower.contains("press") || lower.contains("baro") {
            SensorType::Pressure
        } else if lower.contains("humid") {
            SensorType::Humidity
        } else if lower.contains("temp") {
            SensorType::Temperature
        } else if lower.contains("orient") || lower.contains("rotation") {
            SensorType::Orientation
        } else if lower.contains("grav") {
            SensorType::Gravity
        } else if lower.contains("step") || lower.contains("pedometer") {
            SensorType::StepCounter
        } else if lower.contains("fingerprint") {
            SensorType::Fingerprint
        } else {
            SensorType::Other(name.to_string())
        }
    }

    #[cfg(target_os = "macos")]
    fn refresh_macos(&mut self) -> Result<(), IronError> {
        // macOS has limited IIO-style sensors, mainly on laptops with motion sensors
        // Check for sudden motion sensor
        let text = crate::core::command::capture("system_profiler", &["SPSensorsDataType"])?;
        // Parse ambient light and motion sensors
        for line in text.lines() {
            let line = line.trim();
            if line.contains("Light Sensor") {
                self.items.push(SensorInfo {
                    name: "Ambient Light Sensor".into(),
                    sensor_type: SensorType::AmbientLight,
                    device: "built-in".into(),
                    values: Vec::new(),
                    sampling_frequency_hz: None,
                    active: line.contains("Yes"),
                    vendor: "Apple".into(),
                });
            }
            if line.contains("Motion Sensor") {
                self.items.push(SensorInfo {
                    name: "Sudden Motion Sensor".into(),
                    sensor_type: SensorType::Accelerometer,
                    device: "built-in".into(),
                    values: Vec::new(),
                    sampling_frequency_hz: None,
                    active: line.contains("Yes"),
                    vendor: "Apple".into(),
                });
            }
        }
        Ok(())
    }
}

impl Default for SensorMonitor {
    fn default() -> Self {
        Self::new().unwrap_or(Self {
            items: Vec::new(),
            last_note: Some("sensor enumeration failed and this is the fallback".into()),
        })
    }
}

impl std::fmt::Display for SensorType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Accelerometer => write!(f, "Accelerometer"),
            Self::Gyroscope => write!(f, "Gyroscope"),
            Self::Magnetometer => write!(f, "Magnetometer"),
            Self::AmbientLight => write!(f, "Ambient Light"),
            Self::Proximity => write!(f, "Proximity"),
            Self::Pressure => write!(f, "Pressure"),
            Self::Humidity => write!(f, "Humidity"),
            Self::Temperature => write!(f, "Temperature"),
            Self::Orientation => write!(f, "Orientation"),
            Self::Gravity => write!(f, "Gravity"),
            Self::StepCounter => write!(f, "Step Counter"),
            Self::Fingerprint => write!(f, "Fingerprint"),
            Self::UVIndex => write!(f, "UV Index"),
            Self::ColorSensor => write!(f, "Color"),
            Self::Current => write!(f, "Current"),
            Self::Voltage => write!(f, "Voltage"),
            Self::Other(s) => write!(f, "{}", s),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Constructing the monitor either enumerates or says why it could not.
    ///
    /// This asserted `is_ok()`, which was true by construction until 6.0.0
    /// because `refresh` could not fail. Now that it can, the contract is
    /// weaker and more useful: **whatever happens, the caller can tell which
    /// happened.** A failure has to carry a reason, because a reason is the
    /// whole difference between "this machine has none" and "nobody looked".
    #[test]
    fn test_sensor_monitor_creation() {
        match SensorMonitor::new() {
            Ok(_monitor) => {}
            Err(e) => {
                let why = e.to_string();
                assert!(
                    why.len() > 10,
                    "enumeration failed without saying why: {why:?}"
                );
            }
        }
    }

    #[test]
    fn test_sensor_monitor_default() {
        let monitor = SensorMonitor::default();
        let _ = monitor.sensors();
        let _ = monitor.has_accelerometer();
        let _ = monitor.has_light_sensor();
        let _ = monitor.ambient_light_lux();
    }

    #[test]
    fn test_sensor_value_scaling() {
        let val = SensorValue {
            channel: "x".into(),
            raw: 100.0,
            scale: 0.01,
            offset: 5.0,
            unit: "m/s²".into(),
        };
        assert!((val.scaled_value() - 1.05).abs() < 0.001);
    }

    #[test]
    fn test_sensor_serialization() {
        let sensor = SensorInfo {
            name: "BMI160".into(),
            sensor_type: SensorType::Accelerometer,
            device: "iio:device0".into(),
            values: vec![SensorValue {
                channel: "x".into(),
                raw: 42.0,
                scale: 0.01,
                offset: 0.0,
                unit: "m/s²".into(),
            }],
            sampling_frequency_hz: Some(100.0),
            active: true,
            vendor: "Bosch".into(),
        };
        let json = serde_json::to_string(&sensor).unwrap();
        assert!(json.contains("BMI160"));
        let _: SensorInfo = serde_json::from_str(&json).unwrap();
    }
}
