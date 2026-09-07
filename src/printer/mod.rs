//! Printer and print queue monitoring — connected printers, status, drivers.
//!
//! # Platform Support
//!
//! - **Linux**: Uses `lpstat` (CUPS) to enumerate printers and their status
//! - **Windows**: Uses WMI (`Win32_Printer`) to enumerate printers
//! - **macOS**: Uses `lpstat` (CUPS) to enumerate printers
//!
//! # Examples
//!
//! ```no_run
//! use simonlib::printer::PrinterMonitor;
//!
//! let monitor = PrinterMonitor::new().unwrap();
//! for printer in monitor.printers() {
//!     println!("{}: {} ({})", printer.name, printer.status, printer.driver);
//! }
//! ```

use serde::{Deserialize, Serialize};

use crate::error::SimonError;

/// Printer connection type
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrinterConnection {
    /// Directly attached via USB
    USB,
    /// Network printer (TCP/IP, IPP, etc.)
    Network,
    /// Shared printer via SMB/Windows sharing
    Shared,
    /// Bluetooth connection
    Bluetooth,
    /// Virtual / software printer (e.g., PDF printer)
    Virtual,
    /// Serial / parallel port
    Legacy,
    /// Unknown connection
    Unknown,
}

/// Printer status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrinterStatus {
    /// Ready and idle
    Idle,
    /// Currently printing
    Printing,
    /// Paused / stopped
    Paused,
    /// Offline or not responding
    Offline,
    /// Error condition (paper jam, out of paper, etc.)
    Error,
    /// Unknown status
    Unknown,
}

/// Type of printer
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrinterType {
    /// Laser printer
    Laser,
    /// Inkjet printer
    Inkjet,
    /// Thermal printer (receipt, label)
    Thermal,
    /// Dot matrix / impact
    DotMatrix,
    /// 3D printer
    Printer3D,
    /// Virtual / PDF printer
    Virtual,
    /// Plotter
    Plotter,
    /// Unknown type
    Unknown,
}

/// Information about a single printer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrinterInfo {
    /// Printer name
    pub name: String,
    /// Printer description or model
    pub description: String,
    /// Driver name
    pub driver: String,
    /// Port or URI (e.g., "usb://...", "ipp://...")
    pub port: String,
    /// Connection type
    pub connection: PrinterConnection,
    /// Current status
    pub status: PrinterStatus,
    /// Printer type
    pub printer_type: PrinterType,
    /// Whether this is the default printer
    pub is_default: bool,
    /// Whether the printer accepts new jobs.
    ///
    /// `None` on Windows. CUPS reports it directly -- `lpstat -a` says
    /// "accepting requests" or "not accepting requests" -- and `Win32_Printer`
    /// has no equivalent property. It used to be filled there with
    /// `status != PrinterStatus::Offline`, which is the one derivation the
    /// entity's own description rules out: "distinct from `status`: a stopped
    /// queue may still accept jobs and hold them, which is the difference
    /// between a delayed print and a rejected one".
    pub accepting_jobs: Option<bool>,
    /// Whether the printer is shared over the network
    pub shared: bool,
    /// Whether the printer supports colour. `None` when the platform said
    /// nothing about it, which is not the same as a monochrome printer.
    ///
    /// This was a `bool` filled from a `Color` property that `Win32_Printer`
    /// does not have. It came back null for every printer, `unwrap_or(false)`
    /// made it monochrome, and `system.printer.{n}.color` published that as a
    /// measurement on every Windows machine. Three of the four printers on the
    /// machine that found it do support colour.
    pub color: Option<bool>,
    /// Whether the printer supports duplex (double-sided)
    pub duplex: bool,
    /// Number of jobs in queue
    pub jobs_in_queue: u32,
    /// Location string (if configured)
    pub location: String,
}

/// Monitor for printers and print queues
pub struct PrinterMonitor {
    printers: Vec<PrinterInfo>,
}

/// The queue state, from whichever of the spooler's two enumerations holds one.
///
/// `Win32_Printer` describes the same queue twice and neither column is
/// reliably the specific one. On this machine's four printers, `PrinterStatus`
/// says 3 (Idle) for the three virtual queues while `ExtendedPrinterStatus`
/// says 2 (Unknown) for them -- and for the Brother on a WSD port it is the
/// other way round: `PrinterStatus` says 1 (Other) while
/// `ExtendedPrinterStatus` says 7 (Offline), which the
/// `PRINTER_STATUS_OFFLINE` bit in `PrinterState` confirms.
///
/// Reading only `PrinterStatus` published that printer's state as an absence --
/// "reader returned \"Unknown\", which names an absence rather than a value" --
/// while the spooler was saying "offline" in two other columns, one of which
/// the query already selected and then ignored.
///
/// Neither column is preferred over the other on principle: whichever names a
/// state, answers. 1 (Other) and 2 (Unknown) name an absence rather than a
/// state in both enumerations, and so does any code this crate has no variant
/// for -- those stay `Unknown`, which the resolver reports as unavailable.
#[cfg(target_os = "windows")]
fn status_from_cim(extended: u64, basic: u64) -> PrinterStatus {
    fn specific(code: u64) -> Option<PrinterStatus> {
        match code {
            3 => Some(PrinterStatus::Idle),
            4 => Some(PrinterStatus::Printing),
            // 5 is Warmup and 6 is Stopped Printing in both enumerations; 8 is
            // Paused, and exists in the extended one only.
            5 | 6 | 8 => Some(PrinterStatus::Paused),
            7 => Some(PrinterStatus::Offline),
            _ => None,
        }
    }
    specific(extended)
        .or_else(|| specific(basic))
        .unwrap_or(PrinterStatus::Unknown)
}

impl PrinterMonitor {
    /// Create a new PrinterMonitor and detect printers.
    pub fn new() -> Result<Self, SimonError> {
        let mut monitor = Self {
            printers: Vec::new(),
        };
        monitor.refresh()?;
        Ok(monitor)
    }

    /// Refresh printer detection.
    /// Re-enumerate printers.
    ///
    /// Returns `Err` when the enumeration failed, and `Ok` with an empty list
    /// only when it succeeded and found nothing. See [`crate::core::command`].
    pub fn refresh(&mut self) -> Result<(), SimonError> {
        self.printers.clear();

        #[cfg(target_os = "linux")]
        self.refresh_cups()?;

        #[cfg(target_os = "macos")]
        self.refresh_cups()?;

        #[cfg(target_os = "windows")]
        self.refresh_windows()?;

        Ok(())
    }

    /// Get all detected printers.
    pub fn printers(&self) -> &[PrinterInfo] {
        &self.printers
    }

    /// Get the default printer, if any.
    pub fn default_printer(&self) -> Option<&PrinterInfo> {
        self.printers.iter().find(|p| p.is_default)
    }

    /// Get printers with a specific status.
    pub fn printers_by_status(&self, status: PrinterStatus) -> Vec<&PrinterInfo> {
        self.printers
            .iter()
            .filter(|p| p.status == status)
            .collect()
    }

    /// Get network printers.
    pub fn network_printers(&self) -> Vec<&PrinterInfo> {
        self.printers
            .iter()
            .filter(|p| p.connection == PrinterConnection::Network)
            .collect()
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn refresh_cups(&mut self) -> Result<(), SimonError> {
        // Get default printer
        let default_name = std::process::Command::new("lpstat")
            .args(["-d"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| s.split(':').nth(1).map(|n| n.trim().to_string()))
            .unwrap_or_default();

        // List printers with lpstat -p -l.
        //
        // Three outcomes, and the previous `Err(_) => return` collapsed all of
        // them into "no printers":
        //
        // * `lpstat` is not installed. CUPS is absent, so there are no CUPS
        //   printers -- a reading, and `Ok` with an empty list.
        // * `lpstat` runs and exits non-zero, which is what it does when the
        //   scheduler is not answering. That is not a machine without
        //   printers, and it is what CI runners do.
        // * It runs and prints a list.
        let raw = match std::process::Command::new("lpstat")
            .args(["-p", "-l"])
            .output()
        {
            Ok(o) => o,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(SimonError::CommandFailed(format!("lpstat: {e}"))),
        };
        if !raw.status.success() {
            return Err(SimonError::CommandFailed(format!(
                "lpstat exited {}: {}",
                raw.status,
                String::from_utf8_lossy(&raw.stderr).trim()
            )));
        }
        let output = String::from_utf8(raw.stdout)
            .map_err(|e| SimonError::Parse(format!("lpstat output is not UTF-8: {e}")))?;

        // Parse devices
        let device_output = std::process::Command::new("lpstat")
            .args(["-v"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .unwrap_or_default();

        // Build device URI map: name -> uri
        let mut device_map: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        for line in device_output.lines() {
            // "device for PrinterName: ipp://..."
            if let Some(rest) = line.strip_prefix("device for ") {
                if let Some(colon_pos) = rest.find(':') {
                    let name = rest[..colon_pos].trim().to_string();
                    let uri = rest[colon_pos + 1..].trim().to_string();
                    device_map.insert(name, uri);
                }
            }
        }

        // Get accepting status
        let accept_output = std::process::Command::new("lpstat")
            .args(["-a"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .unwrap_or_default();

        let mut accept_map: std::collections::HashMap<String, bool> =
            std::collections::HashMap::new();
        for line in accept_output.lines() {
            let parts: Vec<&str> = line.splitn(2, ' ').collect();
            if parts.len() >= 2 {
                let accepting = line.contains("accepting");
                accept_map.insert(parts[0].to_string(), accepting);
            }
        }

        // Parse printer blocks
        let mut current_name = String::new();
        let mut current_desc = String::new();
        let mut current_status = PrinterStatus::Unknown;
        let mut in_printer = false;

        for line in output.lines() {
            if let Some(rest) = line.strip_prefix("printer ") {
                // Save previous
                if in_printer && !current_name.is_empty() {
                    let port = device_map.get(&current_name).cloned().unwrap_or_default();
                    let connection = Self::classify_connection_cups(&port);
                    let accepting = accept_map.get(&current_name).copied().unwrap_or(false);
                    self.printers.push(PrinterInfo {
                        name: current_name.clone(),
                        description: current_desc.clone(),
                        driver: String::new(),
                        port,
                        connection,
                        status: current_status.clone(),
                        printer_type: PrinterType::Unknown,
                        is_default: current_name == default_name,
                        accepting_jobs: Some(accepting),
                        shared: false,
                        // Not read on this path.
                        color: None,
                        duplex: false,
                        jobs_in_queue: 0,
                        location: String::new(),
                    });
                }

                // Parse: "printer NAME is idle."  or "printer NAME disabled since ..."
                let name_end = rest
                    .find(" is ")
                    .or_else(|| rest.find(" disabled"))
                    .unwrap_or(rest.len());
                current_name = rest[..name_end].to_string();
                current_desc = String::new();
                in_printer = true;

                if rest.contains("idle") {
                    current_status = PrinterStatus::Idle;
                } else if rest.contains("printing") {
                    current_status = PrinterStatus::Printing;
                } else if rest.contains("disabled") || rest.contains("stopped") {
                    current_status = PrinterStatus::Paused;
                } else {
                    current_status = PrinterStatus::Unknown;
                }
            } else if in_printer && line.starts_with("\tDescription:") {
                current_desc = line
                    .strip_prefix("\tDescription:")
                    .unwrap_or("")
                    .trim()
                    .to_string();
            } else if in_printer && line.starts_with("\tLocation:") {
                // Will store with the printer when we flush it
            }
        }

        // Flush last printer
        if in_printer && !current_name.is_empty() {
            let port = device_map.get(&current_name).cloned().unwrap_or_default();
            let connection = Self::classify_connection_cups(&port);
            let accepting = accept_map.get(&current_name).copied().unwrap_or(false);
            self.printers.push(PrinterInfo {
                name: current_name.clone(),
                description: current_desc,
                driver: String::new(),
                port,
                connection,
                status: current_status,
                printer_type: PrinterType::Unknown,
                is_default: current_name == default_name,
                accepting_jobs: Some(accepting),
                shared: false,
                // Not read on this path.
                color: None,
                duplex: false,
                jobs_in_queue: 0,
                location: String::new(),
            });
        }
        Ok(())
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn classify_connection_cups(uri: &str) -> PrinterConnection {
        let u = uri.to_lowercase();
        if u.starts_with("usb:") {
            PrinterConnection::USB
        } else if u.starts_with("ipp:")
            || u.starts_with("ipps:")
            || u.starts_with("http:")
            || u.starts_with("https:")
            || u.starts_with("socket:")
            || u.starts_with("lpd:")
        {
            PrinterConnection::Network
        } else if u.starts_with("smb:") {
            PrinterConnection::Shared
        } else if u.contains("pdf") || u.contains("cups-pdf") || u.contains("virtual") {
            PrinterConnection::Virtual
        } else if u.starts_with("serial:") || u.starts_with("parallel:") {
            PrinterConnection::Legacy
        } else {
            PrinterConnection::Unknown
        }
    }

    #[cfg(target_os = "windows")]
    fn refresh_windows(&mut self) -> Result<(), SimonError> {
        const QUERY: &str = "Get-CimInstance Win32_Printer | Select-Object Name, DriverName, PortName, PrinterStatus, ExtendedPrinterStatus, Comment, Location, Shared, Capabilities, Default, PrinterState, JobCountSinceLastReset, Network | ConvertTo-Json -Compress";

        let Some(val) =
            crate::core::command::capture_json("powershell", &["-NoProfile", "-Command", QUERY])?
        else {
            return Ok(());
        };
        {
            let items = crate::core::command::json_items(&val);

            for item in &items {
                let name = item["Name"].as_str().unwrap_or("").to_string();
                let driver = item["DriverName"].as_str().unwrap_or("").to_string();
                let port = item["PortName"].as_str().unwrap_or("").to_string();
                let location = item["Location"].as_str().unwrap_or("").to_string();
                let description = item["Comment"].as_str().unwrap_or("").to_string();
                let shared = item["Shared"].as_bool().unwrap_or(false);
                // `Win32_Printer` publishes no `Color` property. Colour lives in
                // `Capabilities`, a `uint16[]` where 2 means Color --- the same
                // array whose `CapabilityDescriptions` spell it out. A printer
                // that reports no capabilities at all has said nothing about
                // colour, and `None` says that.
                let color = item["Capabilities"].as_array().map(|caps| {
                    caps.iter()
                        .filter_map(serde_json::Value::as_u64)
                        .any(|c| c == 2)
                });
                let is_default = item["Default"].as_bool().unwrap_or(false);
                let is_network = item["Network"].as_bool().unwrap_or(false);

                let status = status_from_cim(
                    item["ExtendedPrinterStatus"].as_u64().unwrap_or(0),
                    item["PrinterStatus"].as_u64().unwrap_or(0),
                );

                let port_lower = port.to_lowercase();
                let connection = if is_network {
                    PrinterConnection::Network
                } else if port_lower.starts_with("wsd-") {
                    // A WSD port is Web Services on Devices, which is network
                    // printing by definition -- the port monitor's own name for
                    // the transport, not a word found in the printer's title.
                    // `Win32_Printer.Network` is false for it because the queue
                    // is installed locally, so the only thing that had a view
                    // said nothing and the connection was published absent.
                    PrinterConnection::Network
                } else if port_lower.contains("usb") {
                    PrinterConnection::USB
                } else if name.to_lowercase().contains("pdf")
                    || name.to_lowercase().contains("xps")
                    || name.to_lowercase().contains("onenote")
                    || name.to_lowercase().contains("fax")
                {
                    PrinterConnection::Virtual
                } else {
                    PrinterConnection::Unknown
                };

                let printer_type = Self::infer_printer_type_win(&name, &driver);

                self.printers.push(PrinterInfo {
                    name,
                    description,
                    driver,
                    port,
                    connection,
                    accepting_jobs: None,
                    status,
                    printer_type,
                    is_default,
                    shared,
                    color,
                    duplex: false,
                    jobs_in_queue: 0,
                    location,
                });
            }
        }
        Ok(())
    }

    #[cfg(target_os = "windows")]
    fn infer_printer_type_win(name: &str, driver: &str) -> PrinterType {
        let combined = format!("{} {}", name, driver).to_lowercase();
        if combined.contains("pdf")
            || combined.contains("xps")
            || combined.contains("onenote")
            || combined.contains("fax")
        {
            PrinterType::Virtual
        } else if combined.contains("laser") || combined.contains("laserjet") {
            PrinterType::Laser
        } else if combined.contains("inkjet")
            || combined.contains("deskjet")
            || combined.contains("officejet")
            || combined.contains("pixma")
            || combined.contains("envy")
        {
            PrinterType::Inkjet
        } else if combined.contains("thermal")
            || combined.contains("receipt")
            || combined.contains("label")
        {
            PrinterType::Thermal
        } else if combined.contains("plotter") || combined.contains("designjet") {
            PrinterType::Plotter
        } else {
            PrinterType::Unknown
        }
    }
}

impl Default for PrinterMonitor {
    fn default() -> Self {
        Self::new().unwrap_or(Self {
            printers: Vec::new(),
        })
    }
}

impl std::fmt::Display for PrinterStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Idle => write!(f, "Idle"),
            Self::Printing => write!(f, "Printing"),
            Self::Paused => write!(f, "Paused"),
            Self::Offline => write!(f, "Offline"),
            Self::Error => write!(f, "Error"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

impl std::fmt::Display for PrinterConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::USB => write!(f, "USB"),
            Self::Network => write!(f, "Network"),
            Self::Shared => write!(f, "Shared"),
            Self::Bluetooth => write!(f, "Bluetooth"),
            Self::Virtual => write!(f, "Virtual"),
            Self::Legacy => write!(f, "Legacy"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Constructing the monitor either enumerates printers or says why it
    /// could not. Both are correct answers, and before 6.0.0 only the first
    /// existed: a CI runner with no print scheduler reported zero printers as
    /// a fact about the machine. What must never happen is a silent empty
    /// list, so the failure is required to carry a reason.
    #[test]
    fn test_printer_monitor_creation() {
        match PrinterMonitor::new() {
            Ok(monitor) => {
                let _ = monitor.printers();
            }
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
    fn test_printer_monitor_default() {
        let monitor = PrinterMonitor::default();
        let _ = monitor.printers();
        let _ = monitor.default_printer();
    }

    #[test]
    fn test_printer_serialization() {
        let printer = PrinterInfo {
            name: "TestPrinter".into(),
            description: "A test printer".into(),
            driver: "Generic".into(),
            port: "USB001".into(),
            connection: PrinterConnection::USB,
            status: PrinterStatus::Idle,
            printer_type: PrinterType::Laser,
            is_default: true,
            accepting_jobs: Some(true),
            shared: false,
            color: Some(true),
            duplex: true,
            jobs_in_queue: 0,
            location: "Office".into(),
        };
        let json = serde_json::to_string(&printer).unwrap();
        assert!(json.contains("TestPrinter"));
        let _: PrinterInfo = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn test_status_display() {
        assert_eq!(PrinterStatus::Idle.to_string(), "Idle");
        assert_eq!(PrinterStatus::Printing.to_string(), "Printing");
        assert_eq!(PrinterConnection::USB.to_string(), "USB");
    }
}

#[cfg(all(test, target_os = "windows"))]
mod windows_status_tests {
    use super::{status_from_cim, PrinterStatus};

    /// Codes read off this machine's four queues with
    /// `Get-CimInstance Win32_Printer`.
    #[test]
    fn whichever_column_names_a_state_answers() {
        // The Brother on a WSD port: Other, then Offline.
        assert_eq!(status_from_cim(7, 1), PrinterStatus::Offline);
        // The three virtual queues: Unknown, then Idle.
        assert_eq!(status_from_cim(2, 3), PrinterStatus::Idle);
    }

    #[test]
    fn two_absences_stay_an_absence() {
        // Neither column names a state, so neither is invented. The resolver
        // reports this as unavailable rather than as a queue state.
        assert_eq!(status_from_cim(2, 1), PrinterStatus::Unknown);
        assert_eq!(status_from_cim(0, 0), PrinterStatus::Unknown);
        // A code from a future spooler that this crate has no variant for is
        // an absence too, not a nearest match.
        assert_eq!(status_from_cim(18, 2), PrinterStatus::Unknown);
    }
}
