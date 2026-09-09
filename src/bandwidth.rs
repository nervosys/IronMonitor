//! Bandwidth Testing (iperf-style)
//!
//! Provides network throughput measurement utilities inspired by iperf.
//! Can run as client connecting to a remote server, or measure local
//! network interface throughput.
//!
//! # Examples
//!
//! ## TCP Bandwidth Test
//!
//! ```no_run
//! use ironmonlib::bandwidth::{bandwidth_test, BandwidthConfig, BandwidthResult};
//! use std::time::Duration;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Test download bandwidth from a server
//! let config = BandwidthConfig::default()
//!     .with_duration(Duration::from_secs(5));
//!
//! let result = bandwidth_test("speedtest.example.com", 5201, &config)?;
//! println!("Bandwidth: {:.2} Mbps", result.bandwidth_mbps);
//! # Ok(())
//! # }
//! ```

use crate::error::{IronError, Result};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

/// Default test port (same as iperf3)
pub const DEFAULT_PORT: u16 = 5201;

/// Default test duration in seconds
pub const DEFAULT_DURATION_SECS: u64 = 10;

/// Default buffer size in bytes
pub const DEFAULT_BUFFER_SIZE: usize = 128 * 1024; // 128 KB

/// Bandwidth test configuration
#[derive(Debug, Clone)]
pub struct BandwidthConfig {
    /// Test duration
    pub duration: Duration,
    /// Buffer size for transfers
    pub buffer_size: usize,
    /// Number of parallel streams
    pub parallel_streams: u8,
    /// Connection timeout
    pub connect_timeout: Duration,
    /// Whether to test upload (true) or download (false)
    pub upload: bool,
}

impl Default for BandwidthConfig {
    fn default() -> Self {
        Self {
            duration: Duration::from_secs(DEFAULT_DURATION_SECS),
            buffer_size: DEFAULT_BUFFER_SIZE,
            parallel_streams: 1,
            connect_timeout: Duration::from_secs(10),
            upload: false,
        }
    }
}

impl BandwidthConfig {
    /// Set test duration
    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }

    /// Set buffer size
    pub fn with_buffer_size(mut self, size: usize) -> Self {
        self.buffer_size = size;
        self
    }

    /// Set number of parallel streams
    pub fn with_parallel_streams(mut self, streams: u8) -> Self {
        self.parallel_streams = streams.max(1);
        self
    }

    /// Set connection timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    /// Set upload mode
    pub fn upload_mode(mut self) -> Self {
        self.upload = true;
        self
    }

    /// Set download mode
    pub fn download_mode(mut self) -> Self {
        self.upload = false;
        self
    }
}

/// Bandwidth test result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BandwidthResult {
    /// Target host
    pub host: String,
    /// Target port
    pub port: u16,
    /// Whether this was an upload test
    pub upload: bool,
    /// Test duration in seconds
    pub duration_secs: f64,
    /// Total bytes transferred
    pub bytes_transferred: u64,
    /// Bandwidth in bits per second
    pub bandwidth_bps: f64,
    /// Bandwidth in megabits per second
    pub bandwidth_mbps: f64,
    /// Bandwidth in megabytes per second
    pub bandwidth_mbytes_per_sec: f64,
    /// Number of successful transfers
    pub transfer_count: u64,
    /// Connection time in milliseconds
    pub connect_time_ms: f64,
    /// Whether the test succeeded
    pub success: bool,
    /// Error message (if failed)
    pub error: Option<String>,
}

impl BandwidthResult {
    /// Create a failed result
    fn failed(host: &str, port: u16, error: &str) -> Self {
        Self {
            host: host.to_string(),
            port,
            upload: false,
            duration_secs: 0.0,
            bytes_transferred: 0,
            bandwidth_bps: 0.0,
            bandwidth_mbps: 0.0,
            bandwidth_mbytes_per_sec: 0.0,
            transfer_count: 0,
            connect_time_ms: 0.0,
            success: false,
            error: Some(error.to_string()),
        }
    }
}

/// Perform a bandwidth test to a remote host
///
/// This performs a simple TCP throughput test. For accurate results,
/// the remote host should have an iperf3 server or similar running.
///
/// Note: This is a simplified implementation. For full iperf compatibility,
/// consider using the actual iperf3 binary.
pub fn bandwidth_test(host: &str, port: u16, config: &BandwidthConfig) -> Result<BandwidthResult> {
    let addr = format!("{}:{}", host, port);
    let socket_addr = addr
        .to_socket_addrs()
        .map_err(|e| IronError::Other(format!("Failed to resolve {}: {}", addr, e)))?
        .next()
        .ok_or_else(|| IronError::Other(format!("No addresses found for {}", addr)))?;

    // Measure connection time
    let connect_start = Instant::now();
    let mut stream = TcpStream::connect_timeout(&socket_addr, config.connect_timeout)
        .map_err(|e| IronError::Other(format!("Failed to connect to {}: {}", addr, e)))?;
    let connect_time = connect_start.elapsed();

    // Set socket options
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
    stream.set_write_timeout(Some(Duration::from_secs(5))).ok();
    stream.set_nodelay(true).ok();

    let buffer = vec![0u8; config.buffer_size];
    let mut total_bytes: u64 = 0;
    let mut transfer_count: u64 = 0;

    let test_start = Instant::now();

    if config.upload {
        // Upload test - send data
        while test_start.elapsed() < config.duration {
            match stream.write(&buffer) {
                Ok(n) => {
                    total_bytes += n as u64;
                    transfer_count += 1;
                }
                Err(e) => {
                    if e.kind() != std::io::ErrorKind::WouldBlock {
                        return Ok(BandwidthResult::failed(
                            host,
                            port,
                            &format!("Write error: {}", e),
                        ));
                    }
                }
            }
        }
    } else {
        // Download test - receive data
        let mut read_buffer = vec![0u8; config.buffer_size];
        while test_start.elapsed() < config.duration {
            match stream.read(&mut read_buffer) {
                Ok(0) => break, // Connection closed
                Ok(n) => {
                    total_bytes += n as u64;
                    transfer_count += 1;
                }
                Err(e) => {
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut
                    {
                        continue;
                    }
                    break;
                }
            }
        }
    }

    let duration = test_start.elapsed();
    let duration_secs = duration.as_secs_f64();

    // Calculate bandwidth
    let bits_transferred = total_bytes * 8;
    let bandwidth_bps = if duration_secs > 0.0 {
        bits_transferred as f64 / duration_secs
    } else {
        0.0
    };
    let bandwidth_mbps = bandwidth_bps / 1_000_000.0;
    let bandwidth_mbytes = if duration_secs > 0.0 {
        total_bytes as f64 / duration_secs / 1_000_000.0
    } else {
        0.0
    };

    Ok(BandwidthResult {
        host: host.to_string(),
        port,
        upload: config.upload,
        duration_secs,
        bytes_transferred: total_bytes,
        bandwidth_bps,
        bandwidth_mbps,
        bandwidth_mbytes_per_sec: bandwidth_mbytes,
        transfer_count,
        connect_time_ms: connect_time.as_secs_f64() * 1000.0,
        success: total_bytes > 0,
        error: None,
    })
}

/// Simple local loopback bandwidth test
///
/// Tests how fast data can be transferred through the loopback interface.
/// Useful for measuring system overhead.
pub fn loopback_test(duration: Duration) -> Result<BandwidthResult> {
    use std::net::TcpListener;
    use std::thread;

    // Bind to a random port
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| IronError::Other(e.to_string()))?;
    let port = listener
        .local_addr()
        .map_err(|e| IronError::Other(format!("failed to get local address: {}", e)))?
        .port();

    let buffer_size = DEFAULT_BUFFER_SIZE;
    let test_duration = duration;

    // Spawn receiver thread
    let receiver = thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return 0u64;
        };
        stream
            .set_read_timeout(Some(Duration::from_millis(100)))
            .ok();

        let mut buffer = vec![0u8; buffer_size];
        let mut total_bytes: u64 = 0;
        let start = Instant::now();

        while start.elapsed() < test_duration + Duration::from_secs(1) {
            match stream.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => total_bytes += n as u64,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
                Err(_) => break,
            }
        }

        total_bytes
    });

    // Give the listener time to start
    thread::sleep(Duration::from_millis(50));

    // Connect and send data
    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port))
        .map_err(|e| IronError::Other(e.to_string()))?;
    stream.set_nodelay(true).ok();

    let buffer = vec![0xABu8; buffer_size];
    let mut total_sent: u64 = 0;
    let start = Instant::now();

    while start.elapsed() < test_duration {
        match stream.write(&buffer) {
            Ok(n) => total_sent += n as u64,
            Err(_) => break,
        }
    }

    drop(stream);

    let total_received = receiver.join().unwrap_or(0);
    let duration_secs = start.elapsed().as_secs_f64();

    // Use the minimum of sent/received for conservative estimate
    let bytes_transferred = total_sent.min(total_received);
    let bits_transferred = bytes_transferred * 8;
    let bandwidth_bps = if duration_secs > 0.0 {
        bits_transferred as f64 / duration_secs
    } else {
        0.0
    };

    Ok(BandwidthResult {
        host: "127.0.0.1".to_string(),
        port,
        upload: true,
        duration_secs,
        bytes_transferred,
        bandwidth_bps,
        bandwidth_mbps: bandwidth_bps / 1_000_000.0,
        bandwidth_mbytes_per_sec: bytes_transferred as f64 / duration_secs / 1_000_000.0,
        transfer_count: bytes_transferred / buffer_size as u64,
        connect_time_ms: 0.0,
        success: bytes_transferred > 0,
        error: None,
    })
}

/// Measure memory bandwidth (simple test)
///
/// This performs a basic memory copy test to estimate memory bandwidth.
pub fn memory_bandwidth_test(duration: Duration) -> MemoryBandwidthResult {
    let buffer_size = 64 * 1024 * 1024; // 64 MB
    let src = vec![0xABu8; buffer_size];
    let mut dst = vec![0u8; buffer_size];

    let start = Instant::now();
    let mut iterations = 0u64;
    let mut checksum = 0u8;

    while start.elapsed() < duration {
        dst.copy_from_slice(&src);
        // Prevent optimization by using the result
        checksum = checksum.wrapping_add(dst[iterations as usize % buffer_size]);
        iterations += 1;
    }

    // Use checksum to prevent it being optimized away
    let _ = std::hint::black_box(checksum);

    let duration_secs = start.elapsed().as_secs_f64();
    let total_bytes = iterations * buffer_size as u64 * 2; // Read + Write
    let bandwidth_gbps = if duration_secs > 0.0 {
        (total_bytes as f64 / duration_secs) / 1_000_000_000.0
    } else {
        0.0
    };

    MemoryBandwidthResult {
        duration_secs,
        total_bytes_copied: total_bytes,
        iterations,
        bandwidth_gbytes_per_sec: bandwidth_gbps,
    }
}

/// Memory bandwidth test result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryBandwidthResult {
    /// Test duration in seconds
    pub duration_secs: f64,
    /// Total bytes copied (read + write)
    pub total_bytes_copied: u64,
    /// Number of iterations
    pub iterations: u64,
    /// Bandwidth in GB/s
    pub bandwidth_gbytes_per_sec: f64,
}

/// Quick bandwidth check to common public servers
///
/// Returns estimated download bandwidth in Mbps
pub fn quick_bandwidth_estimate() -> Option<f64> {
    // Try to connect to well-known servers and measure throughput
    let servers = [
        ("8.8.8.8", 53),        // Google DNS
        ("1.1.1.1", 53),        // Cloudflare DNS
        ("208.67.222.222", 53), // OpenDNS
    ];

    let config = BandwidthConfig::default().with_duration(Duration::from_secs(2));

    for (host, port) in &servers {
        if let Ok(result) = bandwidth_test(host, *port, &config) {
            if result.success && result.bandwidth_mbps > 0.0 {
                return Some(result.bandwidth_mbps);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_loopback() {
        let result = loopback_test(Duration::from_secs(1));
        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(result.bandwidth_mbps > 0.0);
        println!("Loopback bandwidth: {:.2} Mbps", result.bandwidth_mbps);
    }

    #[test]
    fn test_memory_bandwidth() {
        let result = memory_bandwidth_test(Duration::from_secs(1));
        assert!(result.bandwidth_gbytes_per_sec > 0.0);
        println!(
            "Memory bandwidth: {:.2} GB/s",
            result.bandwidth_gbytes_per_sec
        );
    }
}
