//! Network Diagnostic Tools (nmap, traceroute, ping, tcpdump, netcat style)
//!
//! This module provides network diagnostic utilities similar to popular CLI tools:
//! - **Ping** - ICMP echo request/reply for connectivity testing
//! - **Traceroute** - Path tracing with hop-by-hop latency
//! - **Port Scanning** - TCP/UDP port discovery (nmap-style)
//! - **Service Detection** - Banner grabbing and version detection
//! - **Packet Capture** - Network traffic capture (tcpdump-style)
//! - **Bandwidth Testing** - Network throughput measurement (iperf-style)
//!
//! # Examples
//!
//! ## Ping a Host
//!
//! ```no_run
//! use ironmonlib::network_tools::{ping, PingResult};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let result = ping("8.8.8.8", 4)?;
//! println!("Ping statistics for {}:", result.host);
//! println!("  Packets: Sent = {}, Received = {}, Lost = {} ({:.0}% loss)",
//!     result.packets_sent, result.packets_received,
//!     result.packets_lost, result.packet_loss_percent);
//! println!("  RTT: min = {:.2}ms, avg = {:.2}ms, max = {:.2}ms",
//!     result.rtt_min_ms, result.rtt_avg_ms, result.rtt_max_ms);
//! # Ok(())
//! # }
//! ```
//!
//! ## Traceroute to Host
//!
//! ```no_run
//! use ironmonlib::network_tools::traceroute;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let result = traceroute("google.com", 30)?;
//! for hop in &result.hops {
//!     if let Some(addr) = &hop.address {
//!         println!("{:>2}  {:15}  {:.2} ms", hop.ttl, addr, hop.rtt_ms.unwrap_or(0.0));
//!     } else {
//!         println!("{:>2}  *", hop.ttl);
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Scan Ports
//!
//! ```no_run
//! use ironmonlib::network_tools::scan_ports;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let results = scan_ports("192.168.1.1", &[22, 80, 443, 8080])?;
//! for result in results {
//!     println!("Port {}: {:?}", result.port, result.status);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Packet Capture (tcpdump-style)
//!
//! ```no_run
//! use ironmonlib::network_tools::{capture_packets, CaptureConfig, CaptureProtocol};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let config = CaptureConfig {
//!     protocol: CaptureProtocol::Tcp,
//!     packet_count: 50,
//!     ..Default::default()
//! };
//!
//! let result = capture_packets(&config)?;
//! println!("Captured {} packets:", result.total_packets);
//! for pkt in &result.packets {
//!     println!("{} {} -> {} {}",
//!         pkt.timestamp, pkt.source, pkt.destination, pkt.protocol);
//! }
//! # Ok(())
//! # }
//! ```

use crate::error::{IronError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr, TcpStream, ToSocketAddrs};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// Ping result statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PingResult {
    /// Target host
    pub host: String,
    /// Resolved IP address
    pub ip_address: Option<String>,
    /// Number of packets sent
    pub packets_sent: u32,
    /// Number of packets received
    pub packets_received: u32,
    /// Number of packets lost
    pub packets_lost: u32,
    /// Packet loss percentage
    pub packet_loss_percent: f64,
    /// Minimum round-trip time in milliseconds
    pub rtt_min_ms: f64,
    /// Maximum round-trip time in milliseconds
    pub rtt_max_ms: f64,
    /// Average round-trip time in milliseconds
    pub rtt_avg_ms: f64,
    /// Standard deviation of RTT (if available)
    pub rtt_stddev_ms: Option<f64>,
    /// Individual ping times
    pub ping_times: Vec<Option<f64>>,
    /// Whether the host is reachable
    pub is_reachable: bool,
}

impl PingResult {
    /// Create a new ping result with default values
    pub fn new(host: &str) -> Self {
        Self {
            host: host.to_string(),
            ip_address: None,
            packets_sent: 0,
            packets_received: 0,
            packets_lost: 0,
            packet_loss_percent: 100.0,
            rtt_min_ms: 0.0,
            rtt_max_ms: 0.0,
            rtt_avg_ms: 0.0,
            rtt_stddev_ms: None,
            ping_times: Vec::new(),
            is_reachable: false,
        }
    }
}

/// Traceroute hop information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TracerouteHop {
    /// TTL value (hop number)
    pub ttl: u8,
    /// IP address of the hop (None if timed out)
    pub address: Option<String>,
    /// Hostname of the hop (if resolved)
    pub hostname: Option<String>,
    /// Round-trip time in milliseconds
    pub rtt_ms: Option<f64>,
    /// Additional RTT measurements
    pub rtt_probes: Vec<Option<f64>>,
    /// Whether this hop responded
    pub responded: bool,
}

/// Traceroute result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TracerouteResult {
    /// Target host
    pub target: String,
    /// Resolved target IP
    pub target_ip: Option<String>,
    /// List of hops
    pub hops: Vec<TracerouteHop>,
    /// Whether the destination was reached
    pub destination_reached: bool,
    /// Total number of hops
    pub total_hops: usize,
}

/// Port scan status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PortStatus {
    /// Port is open and accepting connections
    Open,
    /// Port is closed (connection refused)
    Closed,
    /// Port status unknown (filtered/timeout)
    Filtered,
    /// Error occurred during scan
    Error,
}

impl std::fmt::Display for PortStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PortStatus::Open => write!(f, "open"),
            PortStatus::Closed => write!(f, "closed"),
            PortStatus::Filtered => write!(f, "filtered"),
            PortStatus::Error => write!(f, "error"),
        }
    }
}

/// Port scan result for a single port
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortScanResult {
    /// Port number
    pub port: u16,
    /// Port status
    pub status: PortStatus,
    /// Service name (well-known ports)
    pub service: Option<String>,
    /// Connection time in milliseconds (if open)
    pub connect_time_ms: Option<f64>,
    /// Banner grabbed from service (if available)
    pub banner: Option<String>,
}

/// Network diagnostic tools
pub struct NetworkTools {
    /// Default timeout for operations
    pub timeout: Duration,
}

impl Default for NetworkTools {
    fn default() -> Self {
        Self::new()
    }
}

impl NetworkTools {
    /// Create a new NetworkTools instance
    pub fn new() -> Self {
        Self {
            timeout: Duration::from_secs(3),
        }
    }

    /// Set the timeout for operations
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

/// Ping a host using system ping command
///
/// This function uses the system's ping command to avoid requiring raw socket
/// privileges. It parses the output to extract statistics.
pub fn ping(host: &str, count: u32) -> Result<PingResult> {
    let mut result = PingResult::new(host);
    result.packets_sent = count;

    #[cfg(target_os = "windows")]
    let output = Command::new("ping")
        .args(["-n", &count.to_string(), host])
        .output()
        .map_err(|e| IronError::Other(format!("Failed to execute ping: {}", e)))?;

    #[cfg(not(target_os = "windows"))]
    let output = Command::new("ping")
        .args(["-c", &count.to_string(), host])
        .output()
        .map_err(|e| IronError::Other(format!("Failed to execute ping: {}", e)))?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse ping output
    parse_ping_output(&stdout, &mut result);

    Ok(result)
}

/// Parse ping command output (cross-platform)
fn parse_ping_output(output: &str, result: &mut PingResult) {
    let lines: Vec<&str> = output.lines().collect();

    for line in &lines {
        let line_lower = line.to_lowercase();

        // Parse individual ping times
        if line_lower.contains("time=") || line_lower.contains("time<") {
            // Extract time value
            if let Some(time_start) = line.find("time=").or_else(|| line.find("time<")) {
                let time_str = &line[time_start + 5..];
                if let Some(end) = time_str.find(|c: char| !c.is_ascii_digit() && c != '.') {
                    if let Ok(time) = time_str[..end].parse::<f64>() {
                        result.ping_times.push(Some(time));
                    }
                }
            }
        }

        // Parse statistics line (Windows)
        if line_lower.contains("packets: sent") {
            // Windows format: "Packets: Sent = 4, Received = 4, Lost = 0 (0% loss)"
            if let Some(sent_start) = line.find("Sent = ") {
                let sent_str = &line[sent_start + 7..];
                if let Some(end) = sent_str.find(',') {
                    result.packets_sent = sent_str[..end].trim().parse().unwrap_or(0);
                }
            }
            if let Some(recv_start) = line.find("Received = ") {
                let recv_str = &line[recv_start + 11..];
                if let Some(end) = recv_str.find(',') {
                    result.packets_received = recv_str[..end].trim().parse().unwrap_or(0);
                }
            }
            if let Some(loss_start) = line.find("(") {
                let loss_str = &line[loss_start + 1..];
                if let Some(end) = loss_str.find('%') {
                    result.packet_loss_percent = loss_str[..end].trim().parse().unwrap_or(100.0);
                }
            }
        }

        // Parse statistics line (Linux/macOS)
        if line_lower.contains("packets transmitted") {
            // Format: "4 packets transmitted, 4 received, 0% packet loss"
            let parts: Vec<&str> = line.split(',').collect();
            if !parts.is_empty() {
                if let Some(sent) = parts[0].split_whitespace().next() {
                    result.packets_sent = sent.parse().unwrap_or(0);
                }
            }
            if parts.len() > 1 {
                if let Some(recv) = parts[1].split_whitespace().next() {
                    result.packets_received = recv.parse().unwrap_or(0);
                }
            }
            if parts.len() > 2 {
                if let Some(loss_str) = parts[2].split('%').next() {
                    result.packet_loss_percent = loss_str
                        .split_whitespace()
                        .last()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(100.0);
                }
            }
        }

        // Parse RTT statistics (Windows)
        if line_lower.contains("minimum =") {
            // Format: "Minimum = 10ms, Maximum = 15ms, Average = 12ms"
            if let Some(min_start) = line.find("Minimum = ") {
                let min_str = &line[min_start + 10..];
                if let Some(end) = min_str.find("ms") {
                    result.rtt_min_ms = min_str[..end].trim().parse().unwrap_or(0.0);
                }
            }
            if let Some(max_start) = line.find("Maximum = ") {
                let max_str = &line[max_start + 10..];
                if let Some(end) = max_str.find("ms") {
                    result.rtt_max_ms = max_str[..end].trim().parse().unwrap_or(0.0);
                }
            }
            if let Some(avg_start) = line.find("Average = ") {
                let avg_str = &line[avg_start + 10..];
                if let Some(end) = avg_str.find("ms") {
                    result.rtt_avg_ms = avg_str[..end].trim().parse().unwrap_or(0.0);
                }
            }
        }

        // Parse RTT statistics (Linux/macOS)
        if line_lower.contains("rtt min/avg/max") || line_lower.contains("round-trip min/avg/max") {
            // Format: "rtt min/avg/max/mdev = 10.123/12.456/15.789/2.345 ms"
            if let Some(eq_pos) = line.find('=') {
                let stats_str = line[eq_pos + 1..].trim();
                let parts: Vec<&str> = stats_str.split('/').collect();
                if parts.len() >= 3 {
                    result.rtt_min_ms = parts[0].trim().parse().unwrap_or(0.0);
                    result.rtt_avg_ms = parts[1].trim().parse().unwrap_or(0.0);
                    // Remove "ms" from max value
                    let max_str = parts[2].split_whitespace().next().unwrap_or("0");
                    result.rtt_max_ms = max_str.parse().unwrap_or(0.0);
                    if parts.len() >= 4 {
                        let mdev_str = parts[3].split_whitespace().next().unwrap_or("0");
                        result.rtt_stddev_ms = mdev_str.parse().ok();
                    }
                }
            }
        }
    }

    // Calculate packets lost
    result.packets_lost = result.packets_sent.saturating_sub(result.packets_received);
    result.is_reachable = result.packets_received > 0;

    // If we didn't parse RTT from summary, calculate from individual pings
    if result.rtt_avg_ms == 0.0 && !result.ping_times.is_empty() {
        let valid_times: Vec<f64> = result.ping_times.iter().filter_map(|&t| t).collect();
        if !valid_times.is_empty() {
            result.rtt_min_ms = valid_times.iter().cloned().fold(f64::INFINITY, f64::min);
            result.rtt_max_ms = valid_times.iter().cloned().fold(0.0, f64::max);
            result.rtt_avg_ms = valid_times.iter().sum::<f64>() / valid_times.len() as f64;
        }
    }
}

/// Run traceroute to a host using system command
///
/// Uses the system's traceroute (Linux/macOS) or tracert (Windows) command.
pub fn traceroute(host: &str, max_hops: u8) -> Result<TracerouteResult> {
    let mut result = TracerouteResult {
        target: host.to_string(),
        target_ip: None,
        hops: Vec::new(),
        destination_reached: false,
        total_hops: 0,
    };

    #[cfg(target_os = "windows")]
    let output = Command::new("tracert")
        .args(["-h", &max_hops.to_string(), "-d", host])
        .output()
        .map_err(|e| IronError::Other(format!("Failed to execute tracert: {}", e)))?;

    #[cfg(not(target_os = "windows"))]
    let output = Command::new("traceroute")
        .args(["-m", &max_hops.to_string(), "-n", host])
        .output()
        .map_err(|e| IronError::Other(format!("Failed to execute traceroute: {}", e)))?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse traceroute output
    parse_traceroute_output(&stdout, &mut result);

    Ok(result)
}

/// Parse traceroute command output
fn parse_traceroute_output(output: &str, result: &mut TracerouteResult) {
    let lines: Vec<&str> = output.lines().collect();

    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Skip header lines
        if line.starts_with("traceroute to")
            || line.starts_with("Tracing route")
            || line.contains("maximum")
            || line.contains("over a maximum")
        {
            // Try to extract target IP
            if line.contains('(') && line.contains(')') {
                if let Some(start) = line.find('(') {
                    if let Some(end) = line.find(')') {
                        result.target_ip = Some(line[start + 1..end].to_string());
                    }
                }
            }
            continue;
        }

        // Skip "Trace complete" line
        if line.contains("Trace complete") {
            continue;
        }

        // Parse hop line
        // Windows format: "  1    <1 ms    <1 ms    <1 ms  192.168.1.1"
        // Linux format:   " 1  192.168.1.1  0.543 ms  0.412 ms  0.398 ms"
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        // First part should be TTL number
        if let Ok(ttl) = parts[0].parse::<u8>() {
            let mut hop = TracerouteHop {
                ttl,
                address: None,
                hostname: None,
                rtt_ms: None,
                rtt_probes: Vec::new(),
                responded: false,
            };

            // Check for timeout (all asterisks)
            let has_response = parts.iter().any(|p| !p.contains('*') && p.len() > 1);

            if has_response {
                hop.responded = true;

                // Find IP address
                for part in &parts[1..] {
                    // Check if it's an IP address (contains dots and numbers)
                    if part.chars().all(|c| c.is_ascii_digit() || c == '.') && part.contains('.') {
                        hop.address = Some(part.to_string());
                        break;
                    }
                }

                // Find RTT values (numbers followed by "ms")
                let mut i = 0;
                while i < parts.len() {
                    if parts[i].ends_with("ms") {
                        // RTT value with "ms" suffix
                        let rtt_str = parts[i].trim_end_matches("ms");
                        if let Ok(rtt) = rtt_str.parse::<f64>() {
                            hop.rtt_probes.push(Some(rtt));
                        }
                    } else if i + 1 < parts.len() && parts[i + 1] == "ms" {
                        // RTT value followed by "ms"
                        if let Ok(rtt) = parts[i].parse::<f64>() {
                            hop.rtt_probes.push(Some(rtt));
                        }
                    } else if parts[i].starts_with('<') && parts[i].contains("ms") {
                        // Windows "<1 ms" format
                        let rtt_str = parts[i]
                            .trim_start_matches('<')
                            .trim_end_matches("ms")
                            .trim();
                        if let Ok(rtt) = rtt_str.parse::<f64>() {
                            hop.rtt_probes.push(Some(rtt));
                        }
                    }
                    i += 1;
                }

                // Set primary RTT from first probe
                if !hop.rtt_probes.is_empty() {
                    hop.rtt_ms = hop.rtt_probes[0];
                }

                // Check if this is the destination
                if let Some(ref target_ip) = result.target_ip {
                    if let Some(ref hop_addr) = hop.address {
                        if hop_addr == target_ip {
                            result.destination_reached = true;
                        }
                    }
                }
            }

            result.hops.push(hop);
        }
    }

    result.total_hops = result.hops.len();
}

/// Scan TCP ports on a host (nmap-style)
///
/// Performs TCP connect scan on the specified ports.
/// This is a basic scan that doesn't require raw sockets.
pub fn scan_ports(host: &str, ports: &[u16]) -> Result<Vec<PortScanResult>> {
    scan_ports_with_timeout(host, ports, Duration::from_millis(1000))
}

/// Scan TCP ports with custom timeout
pub fn scan_ports_with_timeout(
    host: &str,
    ports: &[u16],
    timeout: Duration,
) -> Result<Vec<PortScanResult>> {
    let mut results = Vec::with_capacity(ports.len());

    // Resolve hostname
    let addrs: Vec<SocketAddr> = format!("{}:0", host)
        .to_socket_addrs()
        .map_err(|e| IronError::Other(format!("Failed to resolve host: {}", e)))?
        .collect();

    let addr = addrs
        .first()
        .ok_or_else(|| IronError::Other("Could not resolve host".to_string()))?;

    for &port in ports {
        let socket_addr = SocketAddr::new(addr.ip(), port);
        let start = Instant::now();

        let status = match TcpStream::connect_timeout(&socket_addr, timeout) {
            Ok(_stream) => {
                // Connection successful - port is open
                PortStatus::Open
            }
            Err(e) => {
                let err_str = e.to_string().to_lowercase();
                if err_str.contains("refused") || err_str.contains("reset") {
                    PortStatus::Closed
                } else if err_str.contains("timed out") || err_str.contains("timeout") {
                    PortStatus::Filtered
                } else {
                    PortStatus::Error
                }
            }
        };

        let connect_time = if status == PortStatus::Open {
            Some(start.elapsed().as_secs_f64() * 1000.0)
        } else {
            None
        };

        // Grab banner for open ports
        let banner = if status == PortStatus::Open {
            grab_banner(host, port, timeout)
        } else {
            None
        };

        results.push(PortScanResult {
            port,
            status,
            service: get_service_name(port),
            connect_time_ms: connect_time,
            banner,
        });
    }

    Ok(results)
}

/// Scan a range of ports
pub fn scan_port_range(
    host: &str,
    start_port: u16,
    end_port: u16,
    timeout: Duration,
) -> Result<Vec<PortScanResult>> {
    let ports: Vec<u16> = (start_port..=end_port).collect();
    scan_ports_with_timeout(host, &ports, timeout)
}

/// Common ports to scan (similar to nmap's default)
pub fn common_ports() -> Vec<u16> {
    vec![
        20, 21, 22, 23, 25, 53, 67, 68, 69, 80, 110, 119, 123, 135, 137, 138, 139, 143, 161, 162,
        179, 194, 389, 443, 445, 465, 514, 515, 587, 636, 993, 995, 1080, 1433, 1434, 1521, 1723,
        2049, 2082, 2083, 2086, 2087, 2095, 2096, 3306, 3389, 5432, 5900, 5901, 6379, 8080, 8443,
        8888, 9000, 9090, 27017,
    ]
}

/// Parallel port scanning for faster results
///
/// Scans multiple ports concurrently using a thread pool.
/// Much faster than sequential scanning for large port ranges.
///
/// # Arguments
/// * `host` - Target hostname or IP
/// * `ports` - List of ports to scan
/// * `timeout` - Connection timeout per port
/// * `max_threads` - Maximum concurrent threads (default: 100)
///
/// # Example
/// ```no_run
/// use ironmonlib::network_tools::parallel_scan;
/// use std::time::Duration;
///
/// let results = parallel_scan("192.168.1.1", &[22, 80, 443, 8080], Duration::from_secs(1), 50).unwrap();
/// for result in results {
///     if result.status == ironmonlib::network_tools::PortStatus::Open {
///         println!("Port {} is open ({})", result.port, result.service.unwrap_or_default());
///     }
/// }
/// ```
pub fn parallel_scan(
    host: &str,
    ports: &[u16],
    timeout: Duration,
    max_threads: usize,
) -> Result<Vec<PortScanResult>> {
    // Resolve hostname once
    let addrs: Vec<SocketAddr> = format!("{}:0", host)
        .to_socket_addrs()
        .map_err(|e| IronError::Other(format!("Failed to resolve host: {}", e)))?
        .collect();

    let addr = addrs
        .first()
        .ok_or_else(|| IronError::Other("Could not resolve host".to_string()))?;
    let ip = addr.ip();

    // Shared results vector
    let results = Arc::new(Mutex::new(Vec::with_capacity(ports.len())));

    // Process ports in chunks to limit concurrency
    let chunk_size = max_threads.min(ports.len());
    let port_chunks: Vec<Vec<u16>> = ports.chunks(chunk_size).map(|c| c.to_vec()).collect();

    for chunk in port_chunks {
        let mut handles = Vec::with_capacity(chunk.len());

        for port in chunk {
            let results = Arc::clone(&results);

            let handle = thread::spawn(move || {
                let socket_addr = SocketAddr::new(ip, port);
                let start = Instant::now();

                let status = match TcpStream::connect_timeout(&socket_addr, timeout) {
                    Ok(_stream) => PortStatus::Open,
                    Err(e) => {
                        let err_str = e.to_string().to_lowercase();
                        if err_str.contains("refused") || err_str.contains("reset") {
                            PortStatus::Closed
                        } else if err_str.contains("timed out") || err_str.contains("timeout") {
                            PortStatus::Filtered
                        } else {
                            PortStatus::Error
                        }
                    }
                };

                let connect_time = if status == PortStatus::Open {
                    Some(start.elapsed().as_secs_f64() * 1000.0)
                } else {
                    None
                };

                let result = PortScanResult {
                    port,
                    status,
                    service: get_service_name(port),
                    connect_time_ms: connect_time,
                    banner: None,
                };

                let mut results = results.lock().unwrap();
                results.push(result);
            });

            handles.push(handle);
        }

        // Wait for all threads in this chunk
        for handle in handles {
            let _ = handle.join();
        }
    }

    // Extract and sort results by port number
    let mut final_results = Arc::try_unwrap(results)
        .map_err(|_| IronError::Other("Failed to unwrap results".to_string()))?
        .into_inner()
        .map_err(|e| IronError::Other(format!("Mutex error: {}", e)))?;

    final_results.sort_by_key(|r| r.port);
    Ok(final_results)
}

/// Parallel scan with banner grabbing for open ports
pub fn parallel_scan_with_banners(
    host: &str,
    ports: &[u16],
    timeout: Duration,
    max_threads: usize,
) -> Result<Vec<PortScanResult>> {
    let mut results = parallel_scan(host, ports, timeout, max_threads)?;

    // Grab banners for open ports
    for result in &mut results {
        if result.status == PortStatus::Open {
            result.banner = grab_banner(host, result.port, timeout);
        }
    }

    Ok(results)
}

/// Get well-known service name for a port
pub fn get_service_name(port: u16) -> Option<String> {
    let name = match port {
        20 => "ftp-data",
        21 => "ftp",
        22 => "ssh",
        23 => "telnet",
        25 => "smtp",
        53 => "dns",
        67 => "dhcp-server",
        68 => "dhcp-client",
        69 => "tftp",
        80 => "http",
        110 => "pop3",
        119 => "nntp",
        123 => "ntp",
        135 => "msrpc",
        137 => "netbios-ns",
        138 => "netbios-dgm",
        139 => "netbios-ssn",
        143 => "imap",
        161 => "snmp",
        162 => "snmptrap",
        179 => "bgp",
        194 => "irc",
        389 => "ldap",
        443 => "https",
        445 => "microsoft-ds",
        465 => "smtps",
        514 => "syslog",
        515 => "printer",
        587 => "submission",
        636 => "ldaps",
        993 => "imaps",
        995 => "pop3s",
        1080 => "socks",
        1433 => "mssql",
        1434 => "mssql-m",
        1521 => "oracle",
        1723 => "pptp",
        2049 => "nfs",
        3306 => "mysql",
        3389 => "rdp",
        5432 => "postgresql",
        5900 => "vnc",
        5901 => "vnc-1",
        6379 => "redis",
        8080 => "http-proxy",
        8443 => "https-alt",
        8888 => "sun-answerbook",
        9000 => "cslistener",
        9090 => "zeus-admin",
        27017 => "mongodb",
        _ => return None,
    };
    Some(name.to_string())
}

/// Check if a single port is open (netcat-style)
pub fn check_port(host: &str, port: u16, timeout: Duration) -> Result<bool> {
    let addr_str = format!("{}:{}", host, port);
    let addrs: Vec<SocketAddr> = addr_str
        .to_socket_addrs()
        .map_err(|e| IronError::Other(format!("Failed to resolve: {}", e)))?
        .collect();

    if let Some(addr) = addrs.first() {
        match TcpStream::connect_timeout(addr, timeout) {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    } else {
        Ok(false)
    }
}

/// DNS lookup - resolve hostname to IP addresses
pub fn dns_lookup(hostname: &str) -> Result<Vec<IpAddr>> {
    let addrs: Vec<SocketAddr> = format!("{}:0", hostname)
        .to_socket_addrs()
        .map_err(|e| IronError::Other(format!("DNS lookup failed: {}", e)))?
        .collect();

    Ok(addrs.into_iter().map(|a| a.ip()).collect())
}

/// Reverse DNS lookup - resolve IP to hostname
pub fn reverse_dns(ip: &str) -> Result<Option<String>> {
    // Try to parse as IP and do reverse lookup
    if let Ok(_ip_addr) = ip.parse::<IpAddr>() {
        // Use DNS to do reverse lookup (this is platform-specific)
        #[cfg(target_os = "windows")]
        {
            // Windows: Use nslookup or PowerShell
            let output = Command::new("powershell")
                .args([
                    "-Command",
                    &format!("[System.Net.Dns]::GetHostEntry('{}').HostName", ip),
                ])
                .output()
                .ok();

            if let Some(out) = output {
                let hostname = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if !hostname.is_empty() && !hostname.contains("Exception") {
                    return Ok(Some(hostname));
                }
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            // Unix: Use host command
            let output = Command::new("host").arg(ip).output().ok();

            if let Some(out) = output {
                let result = String::from_utf8_lossy(&out.stdout);
                // Parse "x.x.x.x.in-addr.arpa domain name pointer hostname."
                if let Some(line) = result.lines().next() {
                    if line.contains("domain name pointer") {
                        if let Some(hostname) = line.split("pointer").nth(1) {
                            let hostname = hostname.trim().trim_end_matches('.');
                            return Ok(Some(hostname.to_string()));
                        }
                    }
                }
            }
        }
    }

    Ok(None)
}

/// Quick connectivity check to multiple hosts
pub fn check_connectivity(hosts: &[&str]) -> HashMap<String, bool> {
    let mut results = HashMap::new();

    for host in hosts {
        // Try a quick ping (1 packet)
        let reachable = ping(host, 1).map(|r| r.is_reachable).unwrap_or(false);
        results.insert(host.to_string(), reachable);
    }

    results
}

/// Network latency test to a host (similar to mtr)
pub fn latency_test(host: &str, count: u32) -> Result<Vec<f64>> {
    let result = ping(host, count)?;
    Ok(result.ping_times.into_iter().flatten().collect())
}

// ============================================================================
// NMAP-STYLE FUNCTIONALITY
// ============================================================================

/// Service detection result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInfo {
    /// Port number
    pub port: u16,
    /// Service name (e.g., "http", "ssh")
    pub service: String,
    /// Service version (if detected)
    pub version: Option<String>,
    /// Banner/response from service
    pub banner: Option<String>,
    /// Product name (e.g., "Apache", "OpenSSH")
    pub product: Option<String>,
    /// Extra info (e.g., OS, protocol version)
    pub extra_info: Option<String>,
}

/// OS fingerprint result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsFingerprint {
    /// Detected OS family (e.g., "Windows", "Linux")
    pub os_family: Option<String>,
    /// OS version/generation
    pub os_gen: Option<String>,
    /// Confidence percentage (0-100)
    pub confidence: u8,
    /// TTL value observed
    pub ttl: Option<u8>,
    /// TCP window size observed
    pub window_size: Option<u32>,
}

/// Full nmap-style scan result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NmapScanResult {
    /// Target host
    pub host: String,
    /// Resolved IP addresses
    pub ip_addresses: Vec<String>,
    /// Hostname (reverse DNS)
    pub hostname: Option<String>,
    /// Whether host is up
    pub is_up: bool,
    /// Latency in ms
    pub latency_ms: Option<f64>,
    /// Open ports with service info
    pub services: Vec<ServiceInfo>,
    /// OS fingerprint
    pub os_fingerprint: Option<OsFingerprint>,
    /// Scan duration in seconds
    pub scan_duration_secs: f64,
}

/// Grab banner from a service
pub fn grab_banner(host: &str, port: u16, timeout: Duration) -> Option<String> {
    use std::io::{Read, Write};

    let addr_str = format!("{}:{}", host, port);
    let addrs: Vec<SocketAddr> = addr_str.to_socket_addrs().ok()?.collect();
    let addr = addrs.first()?;

    let mut stream = TcpStream::connect_timeout(addr, timeout).ok()?;
    stream.set_read_timeout(Some(timeout)).ok()?;
    stream.set_write_timeout(Some(timeout)).ok()?;

    // Send appropriate probe based on port
    let probe = match port {
        80 | 8080 | 8000 | 8888 => b"HEAD / HTTP/1.0\r\nHost: localhost\r\n\r\n".to_vec(),
        443 | 8443 => return None, // HTTPS needs TLS, skip for now
        21 => vec![],              // FTP sends banner on connect
        22 => vec![],              // SSH sends banner on connect
        25 | 587 => b"EHLO localhost\r\n".to_vec(),
        110 => vec![],                // POP3 sends banner on connect
        143 => vec![],                // IMAP sends banner on connect
        3306 => vec![],               // MySQL sends banner on connect
        5432 => vec![],               // PostgreSQL needs special handling
        6379 => b"INFO\r\n".to_vec(), // Redis
        27017 => vec![],              // MongoDB needs special handling
        _ => vec![],                  // Try to receive without sending
    };

    if !probe.is_empty() {
        let _ = stream.write_all(&probe);
    }

    let mut buffer = [0u8; 2048];
    match stream.read(&mut buffer) {
        Ok(n) if n > 0 => {
            let banner = String::from_utf8_lossy(&buffer[..n])
                .trim()
                .chars()
                .filter(|c| c.is_ascii_graphic() || c.is_ascii_whitespace())
                .take(512)
                .collect::<String>();
            if !banner.is_empty() {
                Some(banner)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Parse banner to detect service version
fn parse_banner_for_version(port: u16, banner: &str) -> ServiceInfo {
    let banner_lower = banner.to_lowercase();
    let mut info = ServiceInfo {
        port,
        service: get_service_name(port).unwrap_or_else(|| "unknown".to_string()),
        version: None,
        banner: Some(banner.chars().take(256).collect()),
        product: None,
        extra_info: None,
    };

    // HTTP detection
    if banner.starts_with("HTTP/") {
        info.service = "http".to_string();
        if let Some(server_line) = banner
            .lines()
            .find(|l| l.to_lowercase().starts_with("server:"))
        {
            let server = server_line[7..].trim();
            info.product = Some(server.to_string());

            // Parse common servers
            if server.contains("Apache") {
                info.product = Some("Apache".to_string());
                if let Some(ver) = extract_version(server, "Apache/") {
                    info.version = Some(ver);
                }
            } else if server.contains("nginx") {
                info.product = Some("nginx".to_string());
                if let Some(ver) = extract_version(server, "nginx/") {
                    info.version = Some(ver);
                }
            } else if server.contains("Microsoft-IIS") {
                info.product = Some("Microsoft IIS".to_string());
                if let Some(ver) = extract_version(server, "Microsoft-IIS/") {
                    info.version = Some(ver);
                }
            }
        }
    }
    // SSH detection
    else if banner_lower.starts_with("ssh-") {
        info.service = "ssh".to_string();
        let parts: Vec<&str> = banner.split_whitespace().collect();
        if !parts.is_empty() {
            info.version = Some(parts[0].to_string());
            if parts.len() > 1 {
                info.product = Some(parts[1..].join(" "));
            }
        }
        if banner_lower.contains("openssh") {
            info.product = Some("OpenSSH".to_string());
        } else if banner_lower.contains("dropbear") {
            info.product = Some("Dropbear".to_string());
        }
    }
    // FTP detection
    else if banner.starts_with("220") {
        info.service = "ftp".to_string();
        if banner_lower.contains("vsftpd") {
            info.product = Some("vsftpd".to_string());
        } else if banner_lower.contains("proftpd") {
            info.product = Some("ProFTPD".to_string());
        } else if banner_lower.contains("filezilla") {
            info.product = Some("FileZilla".to_string());
        } else if banner_lower.contains("microsoft") {
            info.product = Some("Microsoft FTP".to_string());
        }
    }
    // SMTP detection
    else if banner.starts_with("220")
        && (banner_lower.contains("smtp")
            || banner_lower.contains("mail")
            || banner_lower.contains("esmtp"))
    {
        info.service = "smtp".to_string();
        if banner_lower.contains("postfix") {
            info.product = Some("Postfix".to_string());
        } else if banner_lower.contains("sendmail") {
            info.product = Some("Sendmail".to_string());
        } else if banner_lower.contains("exim") {
            info.product = Some("Exim".to_string());
        } else if banner_lower.contains("microsoft") {
            info.product = Some("Microsoft SMTP".to_string());
        }
    }
    // MySQL detection
    else if banner.len() > 4 && banner.as_bytes().get(4) == Some(&0x0a) {
        info.service = "mysql".to_string();
        info.product = Some("MySQL".to_string());
    }
    // Redis detection
    else if banner_lower.contains("redis_version") {
        info.service = "redis".to_string();
        info.product = Some("Redis".to_string());
        if let Some(ver_line) = banner.lines().find(|l| l.starts_with("redis_version:")) {
            info.version = Some(ver_line[14..].to_string());
        }
    }

    info
}

/// Extract version string after a prefix
fn extract_version(s: &str, prefix: &str) -> Option<String> {
    if let Some(start) = s.find(prefix) {
        let after = &s[start + prefix.len()..];
        let version: String = after
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
            .collect();
        if !version.is_empty() {
            return Some(version);
        }
    }
    None
}

/// Guess OS from TTL value
#[allow(dead_code)]
fn guess_os_from_ttl(ttl: u8) -> Option<(&'static str, &'static str, u8)> {
    match ttl {
        // Linux typically uses 64
        60..=64 => Some(("Linux", "2.6+", 85)),
        // Windows typically uses 128
        120..=128 => Some(("Windows", "7/10/11", 85)),
        // Cisco/Network devices typically use 255
        250..=255 => Some(("Network Device", "Cisco/Router", 70)),
        // macOS/BSD typically use 64 but sometimes 255
        _ if ttl > 200 => Some(("BSD/macOS", "", 60)),
        _ => None,
    }
}

/// Perform nmap-style scan on a host
pub fn nmap_scan(host: &str, ports: &[u16], timeout: Duration) -> Result<NmapScanResult> {
    let start = Instant::now();

    // Resolve host
    let ip_addresses: Vec<String> = dns_lookup(host)
        .unwrap_or_default()
        .into_iter()
        .map(|ip| ip.to_string())
        .collect();

    // Check if host is up with ping
    let ping_result = ping(host, 1).ok();
    let is_up = ping_result
        .as_ref()
        .map(|p| p.is_reachable)
        .unwrap_or(false);
    let latency_ms = ping_result
        .as_ref()
        .and_then(|p| p.ping_times.first().copied().flatten());

    // Reverse DNS
    let hostname = ip_addresses
        .first()
        .and_then(|ip| reverse_dns(ip).ok().flatten());

    // Scan ports and grab banners
    let mut services = Vec::new();

    if is_up || !ip_addresses.is_empty() {
        for &port in ports {
            let status = scan_single_port(host, port, timeout);
            if status == PortStatus::Open {
                // Try to grab banner and detect service
                let service_info = if let Some(banner) = grab_banner(host, port, timeout) {
                    parse_banner_for_version(port, &banner)
                } else {
                    ServiceInfo {
                        port,
                        service: get_service_name(port).unwrap_or_else(|| "unknown".to_string()),
                        version: None,
                        banner: None,
                        product: None,
                        extra_info: None,
                    }
                };
                services.push(service_info);
            }
        }
    }

    // Basic OS fingerprinting from ping TTL
    let os_fingerprint = ping_result.as_ref().and_then(|_p| {
        // Try to get TTL from ping (platform-specific, may not be available)
        // For now, use heuristics based on latency and open ports
        let mut os = OsFingerprint {
            os_family: None,
            os_gen: None,
            confidence: 0,
            ttl: None,
            window_size: None,
        };

        // Detect Windows by common ports
        let has_windows_ports = services
            .iter()
            .any(|s| s.port == 135 || s.port == 139 || s.port == 445 || s.port == 3389);
        let has_linux_ports = services.iter().any(|s| s.port == 22);

        if has_windows_ports {
            os.os_family = Some("Windows".to_string());
            os.confidence = 75;
        } else if has_linux_ports {
            os.os_family = Some("Linux".to_string());
            os.confidence = 60;
        }

        // Check service banners for OS hints
        for svc in &services {
            if let Some(ref banner) = svc.banner {
                let banner_lower = banner.to_lowercase();
                if banner_lower.contains("ubuntu") || banner_lower.contains("debian") {
                    os.os_family = Some("Linux".to_string());
                    os.os_gen = Some("Debian/Ubuntu".to_string());
                    os.confidence = 85;
                    break;
                } else if banner_lower.contains("centos")
                    || banner_lower.contains("red hat")
                    || banner_lower.contains("fedora")
                {
                    os.os_family = Some("Linux".to_string());
                    os.os_gen = Some("RHEL/CentOS".to_string());
                    os.confidence = 85;
                    break;
                } else if banner_lower.contains("windows") || banner_lower.contains("microsoft") {
                    os.os_family = Some("Windows".to_string());
                    os.confidence = 85;
                    break;
                }
            }
        }

        if os.confidence > 0 {
            Some(os)
        } else {
            None
        }
    });

    Ok(NmapScanResult {
        host: host.to_string(),
        ip_addresses,
        hostname,
        is_up,
        latency_ms,
        services,
        os_fingerprint,
        scan_duration_secs: start.elapsed().as_secs_f64(),
    })
}

/// Quick scan of common ports (fast nmap alternative)
pub fn quick_scan(host: &str) -> Result<NmapScanResult> {
    let ports = vec![
        21, 22, 23, 25, 53, 80, 110, 139, 143, 443, 445, 993, 995, 3306, 3389, 5432, 8080,
    ];
    nmap_scan(host, &ports, Duration::from_secs(1))
}

/// Full scan of top 1000 ports
pub fn full_scan(host: &str, timeout: Duration) -> Result<NmapScanResult> {
    nmap_scan(host, &common_ports(), timeout)
}

/// Scan single port and return status
fn scan_single_port(host: &str, port: u16, timeout: Duration) -> PortStatus {
    let addr_str = format!("{}:{}", host, port);
    let addrs: Vec<SocketAddr> = match addr_str.to_socket_addrs() {
        Ok(a) => a.collect(),
        Err(_) => return PortStatus::Error,
    };

    if let Some(addr) = addrs.first() {
        match TcpStream::connect_timeout(addr, timeout) {
            Ok(_) => PortStatus::Open,
            Err(e) => {
                let err_str = e.to_string().to_lowercase();
                if err_str.contains("refused") || err_str.contains("reset") {
                    PortStatus::Closed
                } else if err_str.contains("timed out") || err_str.contains("timeout") {
                    // Silence with no refusal is the signature of a firewall dropping
                    // the packet, which is what `Filtered` asserts.
                    PortStatus::Filtered
                } else {
                    // Anything else — host unreachable, network down, permission
                    // denied — says nothing about the port. Reporting those as
                    // `Filtered` claimed a firewall was inferred from an error that
                    // never reached the host.
                    PortStatus::Error
                }
            }
        }
    } else {
        PortStatus::Error
    }
}

// ============================================================================
// TCPDUMP-STYLE PACKET CAPTURE
// ============================================================================

/// Protocol type for packet capture
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CaptureProtocol {
    All,
    Tcp,
    Udp,
    Icmp,
    Arp,
    Http,
    Https,
    Dns,
    Ssh,
}

impl std::fmt::Display for CaptureProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CaptureProtocol::All => write!(f, "all"),
            CaptureProtocol::Tcp => write!(f, "tcp"),
            CaptureProtocol::Udp => write!(f, "udp"),
            CaptureProtocol::Icmp => write!(f, "icmp"),
            CaptureProtocol::Arp => write!(f, "arp"),
            CaptureProtocol::Http => write!(f, "http"),
            CaptureProtocol::Https => write!(f, "https"),
            CaptureProtocol::Dns => write!(f, "dns"),
            CaptureProtocol::Ssh => write!(f, "ssh"),
        }
    }
}

/// Captured packet information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapturedPacket {
    /// Packet number in capture
    pub number: u32,
    /// Timestamp of capture
    pub timestamp: String,
    /// Source IP/MAC address
    pub source: String,
    /// Destination IP/MAC address
    pub destination: String,
    /// Protocol (TCP, UDP, ICMP, etc.)
    pub protocol: String,
    /// Packet length in bytes
    pub length: u32,
    /// Additional info (flags, ports, etc.)
    pub info: String,
    /// Source port (if applicable)
    pub src_port: Option<u16>,
    /// Destination port (if applicable)
    pub dst_port: Option<u16>,
    /// TCP flags (if TCP)
    pub tcp_flags: Option<String>,
    /// Raw packet data (hex, truncated)
    pub data_preview: Option<String>,
}

/// Packet capture configuration
#[derive(Debug, Clone)]
pub struct CaptureConfig {
    /// Network interface to capture on (None = default)
    pub interface: Option<String>,
    /// Filter by protocol
    pub protocol: CaptureProtocol,
    /// Filter by host (source or destination)
    pub host_filter: Option<String>,
    /// Filter by port
    pub port_filter: Option<u16>,
    /// Maximum number of packets to capture
    pub packet_count: u32,
    /// Capture timeout in seconds
    pub timeout_secs: u32,
    /// Include packet data preview
    pub include_data: bool,
    /// Capture filter expression (BPF syntax)
    pub custom_filter: Option<String>,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            interface: None,
            protocol: CaptureProtocol::All,
            host_filter: None,
            port_filter: None,
            packet_count: 100,
            timeout_secs: 30,
            include_data: false,
            custom_filter: None,
        }
    }
}

/// Packet capture result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureResult {
    /// Interface used for capture
    pub interface: String,
    /// Filter expression used
    pub filter: String,
    /// Captured packets
    pub packets: Vec<CapturedPacket>,
    /// Total packets captured
    pub total_packets: u32,
    /// Capture duration in seconds
    pub duration_secs: f64,
    /// Packets per second
    pub packets_per_sec: f64,
    /// Total bytes captured
    pub total_bytes: u64,
    /// Protocol statistics
    pub protocol_stats: HashMap<String, u32>,
    /// Top talkers (by packet count)
    pub top_sources: Vec<(String, u32)>,
    /// Top destinations (by packet count)
    pub top_destinations: Vec<(String, u32)>,
}

/// List available network interfaces for capture
pub fn list_capture_interfaces() -> Result<Vec<String>> {
    #[cfg(target_os = "windows")]
    {
        // Use netsh to list interfaces on Windows
        let output = Command::new("netsh")
            .args(["interface", "show", "interface"])
            .output()
            .map_err(|e| IronError::Other(format!("Failed to list interfaces: {}", e)))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut interfaces = Vec::new();

        for line in stdout.lines().skip(3) {
            // Skip header lines
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 4 {
                // Interface name is the last part (may contain spaces)
                let name = parts[3..].join(" ");
                if !name.is_empty() {
                    interfaces.push(name);
                }
            }
        }

        // Also try PowerShell for more accurate results
        let ps_output = Command::new("powershell")
            .args([
                "-Command",
                "Get-NetAdapter | Where-Object {$_.Status -eq 'Up'} | Select-Object -ExpandProperty Name",
            ])
            .output();

        if let Ok(out) = ps_output {
            let ps_stdout = String::from_utf8_lossy(&out.stdout);
            for line in ps_stdout.lines() {
                let name = line.trim().to_string();
                if !name.is_empty() && !interfaces.contains(&name) {
                    interfaces.push(name);
                }
            }
        }

        Ok(interfaces)
    }

    #[cfg(not(target_os = "windows"))]
    {
        // Use ip or ifconfig on Linux/macOS
        let output = Command::new("ip")
            .args(["link", "show"])
            .output()
            .or_else(|_| Command::new("ifconfig").arg("-a").output())
            .map_err(|e| IronError::Other(format!("Failed to list interfaces: {}", e)))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut interfaces = Vec::new();

        for line in stdout.lines() {
            // Parse "2: eth0: <BROADCAST..." format from ip command
            if line.contains(": <") {
                if let Some(name) = line.split(':').nth(1) {
                    let name = name.trim().to_string();
                    if !name.is_empty() {
                        interfaces.push(name);
                    }
                }
            }
            // Parse ifconfig format
            else if !line.starts_with(' ') && !line.starts_with('\t') && line.contains(':') {
                if let Some(name) = line.split(':').next() {
                    let name = name.trim().to_string();
                    if !name.is_empty() && !interfaces.contains(&name) {
                        interfaces.push(name);
                    }
                }
            }
        }

        Ok(interfaces)
    }
}

/// Build tcpdump/tshark filter expression from config
fn build_capture_filter(config: &CaptureConfig) -> String {
    if let Some(ref custom) = config.custom_filter {
        return custom.clone();
    }

    let mut filters = Vec::new();

    // Protocol filter
    match config.protocol {
        CaptureProtocol::All => {}
        CaptureProtocol::Tcp => filters.push("tcp".to_string()),
        CaptureProtocol::Udp => filters.push("udp".to_string()),
        CaptureProtocol::Icmp => filters.push("icmp".to_string()),
        CaptureProtocol::Arp => filters.push("arp".to_string()),
        CaptureProtocol::Http => filters.push("tcp port 80".to_string()),
        CaptureProtocol::Https => filters.push("tcp port 443".to_string()),
        CaptureProtocol::Dns => filters.push("port 53".to_string()),
        CaptureProtocol::Ssh => filters.push("tcp port 22".to_string()),
    }

    // Host filter
    if let Some(ref host) = config.host_filter {
        filters.push(format!("host {}", host));
    }

    // Port filter
    if let Some(port) = config.port_filter {
        filters.push(format!("port {}", port));
    }

    if filters.is_empty() {
        String::new()
    } else {
        filters.join(" and ")
    }
}

/// Capture packets using system tcpdump/tshark
///
/// **Note**: This function requires elevated privileges (administrator/root)
/// and tcpdump/Wireshark to be installed.
///
/// # Example
/// ```no_run
/// use ironmonlib::network_tools::{capture_packets, CaptureConfig, CaptureProtocol};
///
/// let config = CaptureConfig {
///     protocol: CaptureProtocol::Tcp,
///     host_filter: Some("192.168.1.1".to_string()),
///     packet_count: 50,
///     ..Default::default()
/// };
///
/// match capture_packets(&config) {
///     Ok(result) => {
///         println!("Captured {} packets in {:.2}s", result.total_packets, result.duration_secs);
///         for pkt in &result.packets {
///             println!("{} {} -> {} {} {}",
///                 pkt.timestamp, pkt.source, pkt.destination, pkt.protocol, pkt.info);
///         }
///     }
///     Err(e) => eprintln!("Capture failed: {}", e),
/// }
/// ```
pub fn capture_packets(config: &CaptureConfig) -> Result<CaptureResult> {
    let start = Instant::now();
    let filter = build_capture_filter(config);

    #[cfg(target_os = "windows")]
    {
        capture_packets_windows(config, &filter, start)
    }

    #[cfg(not(target_os = "windows"))]
    {
        capture_packets_unix(config, &filter, start)
    }
}

#[cfg(target_os = "windows")]
fn capture_packets_windows(
    config: &CaptureConfig,
    filter: &str,
    start: Instant,
) -> Result<CaptureResult> {
    // Try tshark (Wireshark CLI) first, then windump
    let tshark_result = capture_with_tshark(config, filter);
    if tshark_result.is_ok() {
        return finalize_capture_result(tshark_result?, start, filter);
    }

    // Fallback to windump
    let windump_result = capture_with_windump(config, filter);
    if windump_result.is_ok() {
        return finalize_capture_result(windump_result?, start, filter);
    }

    // If no capture tool available, try netsh trace (limited)
    capture_with_netsh(config, filter, start)
}

#[cfg(not(target_os = "windows"))]
fn capture_packets_unix(
    config: &CaptureConfig,
    filter: &str,
    start: Instant,
) -> Result<CaptureResult> {
    // Try tcpdump first
    let tcpdump_result = capture_with_tcpdump(config, filter);
    if tcpdump_result.is_ok() {
        return finalize_capture_result(tcpdump_result?, start, filter);
    }

    // Fallback to tshark
    let tshark_result = capture_with_tshark(config, filter);
    if tshark_result.is_ok() {
        return finalize_capture_result(tshark_result?, start, filter);
    }

    Err(IronError::Other(
        "No packet capture tool available. Install tcpdump or Wireshark.".to_string(),
    ))
}

/// Capture using tshark (cross-platform)
fn capture_with_tshark(config: &CaptureConfig, filter: &str) -> Result<Vec<CapturedPacket>> {
    let mut args = vec![
        "-c".to_string(),
        config.packet_count.to_string(),
        "-a".to_string(),
        format!("duration:{}", config.timeout_secs),
        "-T".to_string(),
        "fields".to_string(),
        "-e".to_string(),
        "frame.number".to_string(),
        "-e".to_string(),
        "frame.time_relative".to_string(),
        "-e".to_string(),
        "ip.src".to_string(),
        "-e".to_string(),
        "ip.dst".to_string(),
        "-e".to_string(),
        "_ws.col.Protocol".to_string(),
        "-e".to_string(),
        "frame.len".to_string(),
        "-e".to_string(),
        "tcp.srcport".to_string(),
        "-e".to_string(),
        "tcp.dstport".to_string(),
        "-e".to_string(),
        "tcp.flags".to_string(),
        "-e".to_string(),
        "_ws.col.Info".to_string(),
        "-E".to_string(),
        "separator=|".to_string(),
    ];

    if let Some(ref iface) = config.interface {
        args.push("-i".to_string());
        args.push(iface.clone());
    }

    if !filter.is_empty() {
        args.push("-f".to_string());
        args.push(filter.to_string());
    }

    let output = Command::new("tshark")
        .args(&args)
        .output()
        .map_err(|e| IronError::Other(format!("tshark not found or failed: {}", e)))?;

    if !output.status.success() {
        return Err(IronError::Other(format!(
            "tshark failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    parse_tshark_output(&String::from_utf8_lossy(&output.stdout))
}

fn parse_tshark_output(output: &str) -> Result<Vec<CapturedPacket>> {
    let mut packets = Vec::new();

    for line in output.lines() {
        let parts: Vec<&str> = line.split('|').collect();
        if parts.len() >= 6 {
            let packet = CapturedPacket {
                number: parts[0].parse().unwrap_or(0),
                timestamp: parts[1].to_string(),
                source: parts[2].to_string(),
                destination: parts[3].to_string(),
                protocol: parts[4].to_string(),
                length: parts[5].parse().unwrap_or(0),
                src_port: parts.get(6).and_then(|s| s.parse().ok()),
                dst_port: parts.get(7).and_then(|s| s.parse().ok()),
                tcp_flags: parts
                    .get(8)
                    .map(|s| s.to_string())
                    .filter(|s| !s.is_empty()),
                info: parts.get(9).unwrap_or(&"").to_string(),
                data_preview: None,
            };
            packets.push(packet);
        }
    }

    Ok(packets)
}

#[cfg(not(target_os = "windows"))]
fn capture_with_tcpdump(config: &CaptureConfig, filter: &str) -> Result<Vec<CapturedPacket>> {
    let mut args = vec![
        "-c".to_string(),
        config.packet_count.to_string(),
        "-nn".to_string(),   // Don't resolve names
        "-tttt".to_string(), // Full timestamp
        "-l".to_string(),    // Line buffered
    ];

    if let Some(ref iface) = config.interface {
        args.push("-i".to_string());
        args.push(iface.clone());
    }

    if config.include_data {
        args.push("-X".to_string()); // Include hex dump
    }

    if !filter.is_empty() {
        args.push(filter.to_string());
    }

    let output = Command::new("tcpdump")
        .args(&args)
        .output()
        .map_err(|e| IronError::Other(format!("tcpdump not found or failed: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("permission denied") || stderr.contains("Operation not permitted") {
            return Err(IronError::Other(
                "Permission denied. Run with administrator/root privileges.".to_string(),
            ));
        }
        return Err(IronError::Other(format!("tcpdump failed: {}", stderr)));
    }

    parse_tcpdump_output(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(not(target_os = "windows"))]
fn parse_tcpdump_output(output: &str) -> Result<Vec<CapturedPacket>> {
    let mut packets = Vec::new();
    let mut packet_num = 0u32;

    for line in output.lines() {
        // Skip empty lines and hex dump lines
        if line.is_empty() || line.starts_with('\t') || line.starts_with("0x") {
            continue;
        }

        // Parse tcpdump output format:
        // "2024-01-14 12:00:00.123456 IP 192.168.1.1.443 > 192.168.1.2.54321: Flags [P.], ..."
        let parts: Vec<&str> = line.splitn(4, ' ').collect();
        if parts.len() >= 4 {
            packet_num += 1;

            let timestamp = format!("{} {}", parts[0], parts[1]);
            let proto = parts[2].to_string();

            // Parse the rest of the line for addresses and info
            let rest = parts[3];
            let (source, destination, info) = parse_tcpdump_addresses(rest);

            let packet = CapturedPacket {
                number: packet_num,
                timestamp,
                source,
                destination,
                protocol: proto,
                length: extract_length_from_tcpdump(rest),
                src_port: None, // Parsed from address
                dst_port: None,
                tcp_flags: extract_tcp_flags(rest),
                info,
                data_preview: None,
            };
            packets.push(packet);
        }
    }

    Ok(packets)
}

#[cfg(not(target_os = "windows"))]
fn parse_tcpdump_addresses(line: &str) -> (String, String, String) {
    // Format: "192.168.1.1.443 > 192.168.1.2.54321: Flags [P.], ..."
    let parts: Vec<&str> = line.split(" > ").collect();
    if parts.len() >= 2 {
        let source = parts[0].to_string();
        let rest: Vec<&str> = parts[1].splitn(2, ':').collect();
        let destination = rest[0].to_string();
        let info = rest.get(1).unwrap_or(&"").trim().to_string();
        (source, destination, info)
    } else {
        (String::new(), String::new(), line.to_string())
    }
}

#[cfg(not(target_os = "windows"))]
fn extract_length_from_tcpdump(line: &str) -> u32 {
    // Look for "length X" pattern
    if let Some(idx) = line.find("length ") {
        let after = &line[idx + 7..];
        after
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>()
            .parse()
            .unwrap_or(0)
    } else {
        0
    }
}

fn extract_tcp_flags(line: &str) -> Option<String> {
    // Look for "Flags [X]" pattern
    if let Some(start) = line.find("Flags [") {
        if let Some(end) = line[start..].find(']') {
            return Some(line[start + 7..start + end].to_string());
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn capture_with_windump(config: &CaptureConfig, filter: &str) -> Result<Vec<CapturedPacket>> {
    let mut args = vec![
        "-c".to_string(),
        config.packet_count.to_string(),
        "-nn".to_string(),
        "-tttt".to_string(),
    ];

    if let Some(ref iface) = config.interface {
        args.push("-i".to_string());
        args.push(iface.clone());
    }

    if !filter.is_empty() {
        args.push(filter.to_string());
    }

    let output = Command::new("windump")
        .args(&args)
        .output()
        .map_err(|e| IronError::Other(format!("windump not found: {}", e)))?;

    if !output.status.success() {
        return Err(IronError::Other(format!(
            "windump failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    // Parse windump output (similar to tcpdump)
    parse_windump_output(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(target_os = "windows")]
fn parse_windump_output(output: &str) -> Result<Vec<CapturedPacket>> {
    let mut packets = Vec::new();
    let mut packet_num = 0u32;

    for line in output.lines() {
        if line.is_empty() || line.starts_with('\t') {
            continue;
        }

        // Similar parsing to tcpdump
        let parts: Vec<&str> = line.splitn(4, ' ').collect();
        if parts.len() >= 4 {
            packet_num += 1;

            let packet = CapturedPacket {
                number: packet_num,
                timestamp: format!(
                    "{} {}",
                    parts.first().unwrap_or(&""),
                    parts.get(1).unwrap_or(&"")
                ),
                source: String::new(),
                destination: String::new(),
                protocol: parts.get(2).unwrap_or(&"").to_string(),
                length: 0,
                src_port: None,
                dst_port: None,
                tcp_flags: extract_tcp_flags(parts.get(3).unwrap_or(&"")),
                info: parts.get(3).unwrap_or(&"").to_string(),
                data_preview: None,
            };
            packets.push(packet);
        }
    }

    Ok(packets)
}

#[cfg(target_os = "windows")]
fn capture_with_netsh(
    _config: &CaptureConfig,
    _filter: &str,
    _start: Instant,
) -> Result<CaptureResult> {
    // netsh trace is very limited but available without extra tools
    Err(IronError::Other(
        "No packet capture tool found. Install Wireshark (tshark) or WinDump for packet capture."
            .to_string(),
    ))
}

fn finalize_capture_result(
    packets: Vec<CapturedPacket>,
    start: Instant,
    filter: &str,
) -> Result<CaptureResult> {
    let duration = start.elapsed().as_secs_f64();
    let total_packets = packets.len() as u32;
    let total_bytes: u64 = packets.iter().map(|p| p.length as u64).sum();

    // Calculate protocol statistics
    let mut protocol_stats: HashMap<String, u32> = HashMap::new();
    let mut source_counts: HashMap<String, u32> = HashMap::new();
    let mut dest_counts: HashMap<String, u32> = HashMap::new();

    for pkt in &packets {
        *protocol_stats.entry(pkt.protocol.clone()).or_insert(0) += 1;
        if !pkt.source.is_empty() {
            *source_counts.entry(pkt.source.clone()).or_insert(0) += 1;
        }
        if !pkt.destination.is_empty() {
            *dest_counts.entry(pkt.destination.clone()).or_insert(0) += 1;
        }
    }

    // Sort top talkers
    let mut top_sources: Vec<(String, u32)> = source_counts.into_iter().collect();
    top_sources.sort_by_key(|p| std::cmp::Reverse(p.1));
    top_sources.truncate(10);

    let mut top_destinations: Vec<(String, u32)> = dest_counts.into_iter().collect();
    top_destinations.sort_by_key(|p| std::cmp::Reverse(p.1));
    top_destinations.truncate(10);

    Ok(CaptureResult {
        interface: "default".to_string(),
        filter: filter.to_string(),
        packets,
        total_packets,
        duration_secs: duration,
        packets_per_sec: if duration > 0.0 {
            total_packets as f64 / duration
        } else {
            0.0
        },
        total_bytes,
        protocol_stats,
        top_sources,
        top_destinations,
    })
}

/// Quick packet capture with default settings
pub fn quick_capture(packet_count: u32) -> Result<CaptureResult> {
    capture_packets(&CaptureConfig {
        packet_count,
        timeout_secs: 30,
        ..Default::default()
    })
}

/// Capture only TCP traffic
pub fn capture_tcp(host: Option<&str>, port: Option<u16>, count: u32) -> Result<CaptureResult> {
    capture_packets(&CaptureConfig {
        protocol: CaptureProtocol::Tcp,
        host_filter: host.map(String::from),
        port_filter: port,
        packet_count: count,
        ..Default::default()
    })
}

/// Capture HTTP traffic (port 80)
pub fn capture_http(count: u32) -> Result<CaptureResult> {
    capture_packets(&CaptureConfig {
        protocol: CaptureProtocol::Http,
        packet_count: count,
        ..Default::default()
    })
}

/// Capture DNS traffic (port 53)
pub fn capture_dns(count: u32) -> Result<CaptureResult> {
    capture_packets(&CaptureConfig {
        protocol: CaptureProtocol::Dns,
        packet_count: count,
        ..Default::default()
    })
}

/// Check if packet capture tools are available
pub fn is_capture_available() -> bool {
    #[cfg(target_os = "windows")]
    {
        Command::new("tshark").arg("--version").output().is_ok()
            || Command::new("windump").arg("-h").output().is_ok()
    }

    #[cfg(not(target_os = "windows"))]
    {
        Command::new("tcpdump").arg("--version").output().is_ok()
            || Command::new("tshark").arg("--version").output().is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_service_name() {
        assert_eq!(get_service_name(22), Some("ssh".to_string()));
        assert_eq!(get_service_name(80), Some("http".to_string()));
        assert_eq!(get_service_name(443), Some("https".to_string()));
        assert_eq!(get_service_name(12345), None);
    }

    #[test]
    fn test_common_ports() {
        let ports = common_ports();
        assert!(ports.contains(&22));
        assert!(ports.contains(&80));
        assert!(ports.contains(&443));
    }
}
