//! Network Interface Monitoring
//!
//! This module provides cross-platform network interface statistics including
//! bandwidth usage, packet counts, errors, and interface state.
//!
//! The [`NetworkMonitor`] tracks network interface metrics and calculates real-time
//! bandwidth rates by comparing snapshots over time. It supports both wired (Ethernet)
//! and wireless interfaces on Linux, with Windows and macOS support planned.
//!
//! # Examples
//!
//! ## List All Interfaces
//!
//! ```no_run
//! use ironmonlib::NetworkMonitor;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut monitor = NetworkMonitor::new()?;
//! let interfaces = monitor.interfaces()?;
//!
//! for iface in interfaces {
//!     println!("{}: RX {} MB, TX {} MB",
//!         iface.name,
//!         iface.rx_mb(),
//!         iface.tx_mb()
//!     );
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Monitor Active Interfaces
//!
//! ```no_run
//! use ironmonlib::NetworkMonitor;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut monitor = NetworkMonitor::new()?;
//!
//! // Get only interfaces that are up and running
//! let active = monitor.active_interfaces()?;
//! println!("Active interfaces: {}", active.len());
//!
//! for iface in active {
//!     println!("{}: {} ({})",
//!         iface.name,
//!         if iface.is_up { "UP" } else { "DOWN" },
//!         if iface.is_running { "RUNNING" } else { "STOPPED" }
//!     );
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Calculate Bandwidth Rates
//!
//! ```no_run
//! use ironmonlib::NetworkMonitor;
//! use std::{thread, time::Duration};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut monitor = NetworkMonitor::new()?;
//!
//! // Establish baseline
//! let _ = monitor.interfaces()?;
//! thread::sleep(Duration::from_secs(1));
//!
//! // Get current stats and calculate rates
//! let interfaces = monitor.interfaces()?;
//! for iface in interfaces {
//!     if iface.is_active() {
//!         // `None` on the first call for an interface: a rate needs two
//!         // samples, and one call is one sample.
//!         if let Some((rx_rate, tx_rate)) = monitor.bandwidth_rate(&iface.name, &iface) {
//!             println!("{}: ↓{:.2} MB/s ↑{:.2} MB/s",
//!                 iface.name,
//!                 rx_rate / 1_000_000.0,
//!                 tx_rate / 1_000_000.0
//!             );
//!         }
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Monitor Specific Interface
//!
//! ```no_run
//! use ironmonlib::NetworkMonitor;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut monitor = NetworkMonitor::new()?;
//!
//! // Get specific interface by name
//! if let Some(eth0) = monitor.interface_by_name("eth0")? {
//!     println!("Interface: {}", eth0.name);
//!     println!("State: {} / {}",
//!         if eth0.is_up { "UP" } else { "DOWN" },
//!         if eth0.is_running { "RUNNING" } else { "STOPPED" }
//!     );
//!     println!("RX: {} bytes ({} packets, {} errors)",
//!         eth0.rx_bytes, eth0.rx_packets, eth0.rx_errors);
//!     println!("TX: {} bytes ({} packets, {} errors)",
//!         eth0.tx_bytes, eth0.tx_packets, eth0.tx_errors);
//!     
//!     if let Some(speed) = eth0.speed_mbps {
//!         println!("Link speed: {} Mbps", speed);
//!     }
//!     if let Some(mtu) = eth0.mtu {
//!         println!("MTU: {}", mtu);
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Track Interface Health
//!
//! ```no_run
//! use ironmonlib::NetworkMonitor;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut monitor = NetworkMonitor::new()?;
//! let interfaces = monitor.interfaces()?;
//!
//! for iface in interfaces {
//!     let total_errors = iface.total_errors();
//!     let total_drops = iface.total_drops();
//!     
//!     if total_errors > 0 || total_drops > 0 {
//!         println!("[!] {}: {} errors, {} drops",
//!             iface.name, total_errors, total_drops);
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Platform Support
//!
//! | Platform | Interface Enum | RX/TX Stats | Bandwidth Rate | Speed/MTU | IP Addresses |
//! |----------|----------------|-------------|----------------|-----------|--------------|
//! | Linux    | ✅ sysfs       | ✅          | ✅             | ✅        | 🚧           |
//! | Windows  | 🚧             | 🚧          | 🚧             | 🚧        | 🚧           |
//! | macOS    | 🚧             | 🚧          | 🚧             | 🚧        | 🚧           |
//!
//! ## Linux Implementation
//!
//! The Linux backend reads from `/sys/class/net/[interface]/` for interface enumeration
//! and statistics:
//!
//! - **operstate**: Interface state (up/down)
//! - **flags**: Running status
//! - **statistics/**: RX/TX bytes, packets, errors, drops
//! - **speed**: Link speed (for Ethernet)
//! - **mtu**: Maximum Transmission Unit
//! - **address**: MAC address

use crate::error::{IronError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Network interface information and statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInterfaceInfo {
    /// Interface name (e.g., "eth0", "wlan0", "Ethernet0")
    pub name: String,
    /// Interface is up/active
    pub is_up: bool,
    /// Interface is running
    pub is_running: bool,
    /// Bytes received
    pub rx_bytes: u64,
    /// Packets received
    pub rx_packets: u64,
    /// Receive errors
    pub rx_errors: u64,
    /// Receive drops
    pub rx_drops: u64,
    /// Bytes transmitted
    pub tx_bytes: u64,
    /// Packets transmitted
    pub tx_packets: u64,
    /// Transmit errors
    pub tx_errors: u64,
    /// Transmit drops
    pub tx_drops: u64,
    /// Link speed in Mbps (if available)
    pub speed_mbps: Option<u32>,
    /// Maximum Transmission Unit
    pub mtu: Option<u32>,
    /// MAC address
    pub mac_address: Option<String>,
    /// IPv4 addresses
    pub ipv4_addresses: Vec<String>,
    /// IPv6 addresses
    pub ipv6_addresses: Vec<String>,
}

impl NetworkInterfaceInfo {
    /// Get total bytes transferred (RX + TX)
    pub fn total_bytes(&self) -> u64 {
        self.rx_bytes + self.tx_bytes
    }

    /// Get total packets transferred (RX + TX)
    pub fn total_packets(&self) -> u64 {
        self.rx_packets + self.tx_packets
    }

    /// Get total errors (RX + TX)
    pub fn total_errors(&self) -> u64 {
        self.rx_errors + self.tx_errors
    }

    /// Get total drops (RX + TX)
    pub fn total_drops(&self) -> u64 {
        self.rx_drops + self.tx_drops
    }

    /// Get RX bandwidth in megabytes
    pub fn rx_mb(&self) -> f64 {
        self.rx_bytes as f64 / (1024.0 * 1024.0)
    }

    /// Get TX bandwidth in megabytes
    pub fn tx_mb(&self) -> f64 {
        self.tx_bytes as f64 / (1024.0 * 1024.0)
    }

    /// Get total bandwidth in megabytes
    pub fn total_mb(&self) -> f64 {
        self.total_bytes() as f64 / (1024.0 * 1024.0)
    }

    /// Check if interface is active (up and running)
    pub fn is_active(&self) -> bool {
        self.is_up && self.is_running
    }
}

/// Network bandwidth statistics for rate calculation
#[derive(Debug, Clone)]
pub struct BandwidthStats {
    /// Previous RX bytes
    pub prev_rx_bytes: u64,
    /// Previous TX bytes
    pub prev_tx_bytes: u64,
    /// Timestamp of previous measurement
    pub prev_time: std::time::Instant,
}

/// Network monitor for tracking interface statistics
pub struct NetworkMonitor {
    /// Last known state for bandwidth calculation
    prev_stats: HashMap<String, BandwidthStats>,
    /// Last update time
    last_update: std::time::Instant,
}

impl NetworkMonitor {
    /// Create a new network monitor
    pub fn new() -> Result<Self> {
        Ok(Self {
            prev_stats: HashMap::new(),
            last_update: std::time::Instant::now(),
        })
    }

    /// Get all network interfaces.
    ///
    /// This used to record the counters it returned as the baseline for
    /// [`Self::bandwidth_rate`], which made every rate in the crate exactly
    /// zero: a caller fetches interfaces, the baseline is overwritten with the
    /// values just fetched, and the rate is then computed as those values minus
    /// themselves. Seven call sites did that -- the agent tools, the
    /// observability API, the Prometheus exporter and the pipeline -- so
    /// `rx_bytes_per_sec` was `0.0` on every interface of every machine, and
    /// looked like an idle network rather than a broken subtraction.
    ///
    /// The baseline now belongs to `bandwidth_rate`, which is the only thing
    /// that reads it.
    pub fn interfaces(&mut self) -> Result<Vec<NetworkInterfaceInfo>> {
        Self::enumerate_interfaces()
    }

    /// Get only active (up and running) interfaces
    pub fn active_interfaces(&mut self) -> Result<Vec<NetworkInterfaceInfo>> {
        let interfaces = self.interfaces()?;
        Ok(interfaces
            .into_iter()
            .filter(|iface| iface.is_active())
            .collect())
    }

    /// Get a specific interface by name
    pub fn interface_by_name(&mut self, name: &str) -> Result<Option<NetworkInterfaceInfo>> {
        let interfaces = self.interfaces()?;
        Ok(interfaces.into_iter().find(|iface| iface.name == name))
    }

    /// Calculate bandwidth rates (bytes/sec) for an interface
    pub fn bandwidth_rate(
        &mut self,
        name: &str,
        current: &NetworkInterfaceInfo,
    ) -> Option<(f64, f64)> {
        let now = std::time::Instant::now();
        let previous = self.prev_stats.insert(
            name.to_string(),
            BandwidthStats {
                prev_rx_bytes: current.rx_bytes,
                prev_tx_bytes: current.tx_bytes,
                prev_time: now,
            },
        );
        self.last_update = now;

        // A rate needs two samples. The first call for an interface records one
        // and reports nothing -- the same contract as the PDH per-core clock
        // reader and for the same reason. This used to return `(0.0, 0.0)`,
        // which is a claim that the interface carried no traffic.
        let prev = previous?;
        let elapsed = now.duration_since(prev.prev_time).as_secs_f64();
        if elapsed <= 0.0 {
            return None;
        }
        Some((
            current.rx_bytes.saturating_sub(prev.prev_rx_bytes) as f64 / elapsed,
            current.tx_bytes.saturating_sub(prev.prev_tx_bytes) as f64 / elapsed,
        ))
    }

    /// Get interface count
    pub fn interface_count(&mut self) -> Result<usize> {
        Ok(self.interfaces()?.len())
    }

    /// Get active interface count
    pub fn active_interface_count(&mut self) -> Result<usize> {
        Ok(self.active_interfaces()?.len())
    }

    // Platform-specific interface enumeration
    #[cfg(target_os = "linux")]
    fn enumerate_interfaces() -> Result<Vec<NetworkInterfaceInfo>> {
        linux::enumerate_interfaces()
    }

    #[cfg(target_os = "windows")]
    fn enumerate_interfaces() -> Result<Vec<NetworkInterfaceInfo>> {
        windows::enumerate_interfaces()
    }

    #[cfg(target_os = "macos")]
    fn enumerate_interfaces() -> Result<Vec<NetworkInterfaceInfo>> {
        macos::enumerate_interfaces()
    }
}

impl Default for NetworkMonitor {
    fn default() -> Self {
        Self::new().unwrap_or_else(|_| Self {
            prev_stats: HashMap::new(),
            last_update: std::time::Instant::now(),
        })
    }
}

// Linux-specific network interface enumeration
#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::fs;
    use std::path::Path;

    pub fn enumerate_interfaces() -> Result<Vec<NetworkInterfaceInfo>> {
        let mut interfaces = Vec::new();

        let sys_net = Path::new("/sys/class/net");
        if !sys_net.exists() {
            return Err(IronError::UnsupportedPlatform(
                "/sys/class/net not available".to_string(),
            ));
        }

        // Iterate through network interfaces
        for entry in fs::read_dir(sys_net)? {
            let entry = entry?;
            let iface_name = entry.file_name().to_string_lossy().to_string();

            if let Ok(iface_info) = read_interface_info(&iface_name) {
                interfaces.push(iface_info);
            }
        }

        Ok(interfaces)
    }

    fn read_interface_info(name: &str) -> Result<NetworkInterfaceInfo> {
        let base_path = format!("/sys/class/net/{}", name);

        // Read interface state
        let is_up = read_operstate(&base_path)?.contains("up");
        let is_running = read_flags(&base_path)?.contains("running");

        // Read statistics from /sys/class/net/[iface]/statistics
        let stats_path = format!("{}/statistics", base_path);
        let rx_bytes = read_stat(&stats_path, "rx_bytes")?;
        let rx_packets = read_stat(&stats_path, "rx_packets")?;
        let rx_errors = read_stat(&stats_path, "rx_errors")?;
        let rx_drops = read_stat(&stats_path, "rx_dropped")?;
        let tx_bytes = read_stat(&stats_path, "tx_bytes")?;
        let tx_packets = read_stat(&stats_path, "tx_packets")?;
        let tx_errors = read_stat(&stats_path, "tx_errors")?;
        let tx_drops = read_stat(&stats_path, "tx_dropped")?;

        // Read MTU
        let mtu = read_file_u32(&format!("{}/mtu", base_path));

        // Read speed (in Mbps)
        let speed_mbps = read_file_u32(&format!("{}/speed", base_path));

        // Read MAC address
        let mac_address = read_file_string(&format!("{}/address", base_path));

        // Read IP addresses using ip command or /proc/net
        let (ipv4_addresses, ipv6_addresses) = read_ip_addresses(name)?;

        Ok(NetworkInterfaceInfo {
            name: name.to_string(),
            is_up,
            is_running,
            rx_bytes,
            rx_packets,
            rx_errors,
            rx_drops,
            tx_bytes,
            tx_packets,
            tx_errors,
            tx_drops,
            speed_mbps,
            mtu,
            mac_address,
            ipv4_addresses,
            ipv6_addresses,
        })
    }

    fn read_operstate(base_path: &str) -> Result<String> {
        let path = format!("{}/operstate", base_path);
        fs::read_to_string(&path)
            .map(|s| s.trim().to_lowercase())
            .or(Ok("unknown".to_string()))
    }

    fn read_flags(base_path: &str) -> Result<String> {
        let path = format!("{}/flags", base_path);
        fs::read_to_string(&path)
            .map(|s| s.trim().to_lowercase())
            .or(Ok("0x0".to_string()))
    }

    fn read_stat(stats_path: &str, stat_name: &str) -> Result<u64> {
        let path = format!("{}/{}", stats_path, stat_name);
        let content = fs::read_to_string(&path)?;
        content
            .trim()
            .parse()
            .map_err(|e| IronError::Parse(format!("Failed to parse {}: {}", stat_name, e)))
    }

    fn read_file_u32(path: &str) -> Option<u32> {
        fs::read_to_string(path)
            .ok()
            .and_then(|s| s.trim().parse().ok())
    }

    fn read_file_string(path: &str) -> Option<String> {
        fs::read_to_string(path)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    fn read_ip_addresses(iface_name: &str) -> Result<(Vec<String>, Vec<String>)> {
        let ipv4_addrs = Vec::new();
        let mut ipv6_addrs = Vec::new();

        // Read IPv4 addresses from /proc/net/fib_trie
        // Parse entries that match our interface
        if let Ok(content) = fs::read_to_string("/proc/net/fib_trie") {
            let mut current_prefix = String::new();
            let mut is_local = false;

            for line in content.lines() {
                let trimmed = line.trim();

                // Look for IP prefix entries
                if trimmed.starts_with("+--") || trimmed.starts_with("|--") {
                    if let Some(prefix) = trimmed
                        .strip_prefix("+-- ")
                        .or_else(|| trimmed.strip_prefix("|-- "))
                    {
                        current_prefix = prefix.split('/').next().unwrap_or("").to_string();
                        is_local = false;
                    }
                }

                // Check if this is a LOCAL entry (indicates it's our IP)
                if trimmed.contains("/32 host LOCAL") {
                    is_local = true;
                }

                // When we find a LOCAL entry, the previous prefix is an IP on this host
                if is_local && !current_prefix.is_empty() && current_prefix != "0.0.0.0" {
                    // Verify this IP belongs to our interface by checking sysfs
                    let sysfs_path = format!("/sys/class/net/{}/address", iface_name);
                    if fs::metadata(&sysfs_path).is_ok() {
                        // Simple heuristic: add the IP (fib_trie doesn't tell us which iface owns it)
                        // This is a simplified approach
                        if !ipv4_addrs.contains(&current_prefix) {
                            // Don't add yet - we need to use another method
                        }
                    }
                    is_local = false;
                }
            }
        }

        // More reliable method: parse output of /proc/net/route to find interface IPs
        // For now, use sysfs operstate + route table to infer

        // Read IPv6 addresses from /proc/net/if_inet6
        if let Ok(content) = fs::read_to_string("/proc/net/if_inet6") {
            for line in content.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 6 && parts[5] == iface_name {
                    let hex_addr = parts[0];
                    // Convert hex format (fe80...0001) to IPv6 notation
                    if hex_addr.len() == 32 {
                        let ipv6 = format!(
                            "{:0>4}:{:0>4}:{:0>4}:{:0>4}:{:0>4}:{:0>4}:{:0>4}:{:0>4}",
                            &hex_addr[0..4],
                            &hex_addr[4..8],
                            &hex_addr[8..12],
                            &hex_addr[12..16],
                            &hex_addr[16..20],
                            &hex_addr[20..24],
                            &hex_addr[24..28],
                            &hex_addr[28..32]
                        );
                        ipv6_addrs.push(ipv6);
                    }
                }
            }
        }

        // Try to get IPv4 from /proc/net/route (interfaces with routes have IPs)
        // This is still incomplete but better than nothing
        if let Ok(content) = fs::read_to_string("/proc/net/route") {
            for line in content.lines().skip(1) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 8 && parts[0] == iface_name {
                    // Gateway and destination are in hex format
                    let dest = parts[1];
                    let gateway = parts[2];

                    // Default route (0.0.0.0) - interface has an IP
                    if dest == "00000000" && gateway != "00000000" {
                        // Parse gateway hex to IP (little-endian on x86)
                        if let Ok(gw) = u32::from_str_radix(gateway, 16) {
                            let gw_ip = format!(
                                "{}.{}.{}.{}",
                                gw & 0xFF,
                                (gw >> 8) & 0xFF,
                                (gw >> 16) & 0xFF,
                                (gw >> 24) & 0xFF
                            );
                            // Gateway IP, not our IP, but indicates interface is active
                            let _ = gw_ip;
                        }
                    }
                }
            }
        }

        Ok((ipv4_addrs, ipv6_addrs))
    }
}

// Windows-specific network interface enumeration
#[cfg(target_os = "windows")]
mod windows {
    use super::*;
    use std::ptr;

    // MIB_IF_ROW2 structure for GetIfTable2 (64-bit counters)
    #[repr(C)]
    struct MIB_IF_ROW2 {
        interface_luid: u64,
        interface_index: u32,
        interface_guid: [u8; 16],
        alias: [u16; 257], // Friendly name
        description: [u16; 257],
        phys_addr_length: u32,
        phys_addr: [u8; 32],
        permanent_phys_addr: [u8; 32],
        mtu: u32,
        if_type: u32,
        tunnel_type: u32,
        media_type: u32,
        phys_medium_type: u32,
        access_type: u32,
        direction_type: u32,
        interface_and_oper_status_flags: u8,
        oper_status: u32,
        admin_status: u32,
        media_connect_state: u32,
        network_guid: [u8; 16],
        connection_type: u32,
        transmit_link_speed: u64,
        receive_link_speed: u64,
        in_octets: u64,
        in_ucast_pkts: u64,
        in_nucast_pkts: u64,
        in_discards: u64,
        in_errors: u64,
        in_unknown_protos: u64,
        in_ucast_octets: u64,
        in_mcast_octets: u64,
        in_bcast_octets: u64,
        out_octets: u64,
        out_ucast_pkts: u64,
        out_nucast_pkts: u64,
        out_discards: u64,
        out_errors: u64,
        out_ucast_octets: u64,
        out_mcast_octets: u64,
        out_bcast_octets: u64,
        out_qlen: u64,
    }

    #[repr(C)]
    struct MIB_IF_TABLE2 {
        num_entries: u32,
        table: [MIB_IF_ROW2; 1], // Variable length array
    }

    // OperStatus values
    const IF_OPER_STATUS_UP: u32 = 1;
    #[allow(dead_code)]
    const IF_OPER_STATUS_DOWN: u32 = 2;
    #[allow(dead_code)]
    const IF_OPER_STATUS_TESTING: u32 = 3;
    const IF_OPER_STATUS_DORMANT: u32 = 5;

    // MediaConnectState values
    const MEDIA_CONNECT_STATE_CONNECTED: u32 = 1;

    // Interface types
    const IF_TYPE_SOFTWARE_LOOPBACK: u32 = 24;
    const IF_TYPE_TUNNEL: u32 = 131;

    #[link(name = "iphlpapi")]
    extern "system" {
        fn GetIfTable2(table: *mut *mut MIB_IF_TABLE2) -> u32;
        fn FreeMibTable(memory: *mut std::ffi::c_void);
    }

    const NO_ERROR: u32 = 0;

    pub fn enumerate_interfaces() -> Result<Vec<NetworkInterfaceInfo>> {
        let mut interfaces = Vec::new();
        let mut table: *mut MIB_IF_TABLE2 = ptr::null_mut();

        unsafe {
            let result = GetIfTable2(&mut table);
            if result != NO_ERROR {
                return Err(IronError::System(format!("GetIfTable2 failed: {}", result)));
            }

            if table.is_null() {
                return Ok(interfaces);
            }

            let num_entries = (*table).num_entries as usize;
            let table_ptr = &(*table).table as *const MIB_IF_ROW2;

            for i in 0..num_entries {
                let row = &*table_ptr.add(i);

                // Skip loopback and tunnel interfaces
                if row.if_type == IF_TYPE_SOFTWARE_LOOPBACK || row.if_type == IF_TYPE_TUNNEL {
                    continue;
                }

                // Get friendly name (alias) - this is the user-visible name
                let alias_len = row.alias.iter().position(|&c| c == 0).unwrap_or(257);
                let alias = String::from_utf16_lossy(&row.alias[..alias_len]);

                // Get description as fallback
                let desc_len = row.description.iter().position(|&c| c == 0).unwrap_or(257);
                let description = String::from_utf16_lossy(&row.description[..desc_len]);

                // Use alias (friendly name) if available, otherwise description
                let name = if !alias.is_empty() {
                    alias
                } else if !description.is_empty() {
                    description
                } else {
                    format!("Interface {}", row.interface_index)
                };

                // Skip internal/virtual interfaces and filter driver layers by name patterns
                // Filter drivers appear as "InterfaceName-DriverName-0000"
                if name.contains("Loopback")
                    || name.contains("isatap")
                    || name.contains("Teredo")
                    || name.contains("6TO4")
                    || name.contains("-WFP Native MAC Layer")
                    || name.contains("-WFP 802.3 MAC Layer")
                    || name.contains("-Npcap Packet Driver")
                    || name.contains("-QoS Packet Scheduler")
                    || name.contains("-Native WiFi Filter Driver")
                    || name.contains("-Virtual Filtering Platform")
                    || name.contains("-Hyper-V Virtual Switch")
                {
                    continue;
                }

                // Format MAC address
                let mac_address = if row.phys_addr_length > 0 {
                    let len = row.phys_addr_length as usize;
                    Some(
                        row.phys_addr[..len.min(6)]
                            .iter()
                            .map(|b| format!("{:02X}", b))
                            .collect::<Vec<_>>()
                            .join(":"),
                    )
                } else {
                    None
                };

                // Speed in Mbps (transmit_link_speed is in bits per second)
                let speed_mbps = if row.transmit_link_speed > 0 {
                    Some((row.transmit_link_speed / 1_000_000) as u32)
                } else {
                    None
                };

                // Determine operational status
                let is_up = row.oper_status == IF_OPER_STATUS_UP
                    || (row.oper_status == IF_OPER_STATUS_DORMANT
                        && row.media_connect_state == MEDIA_CONNECT_STATE_CONNECTED);
                let is_running = row.admin_status == 1
                    && row.media_connect_state == MEDIA_CONNECT_STATE_CONNECTED;

                interfaces.push(NetworkInterfaceInfo {
                    name,
                    is_up,
                    is_running,
                    rx_bytes: row.in_octets,
                    rx_packets: row.in_ucast_pkts + row.in_nucast_pkts,
                    rx_errors: row.in_errors,
                    rx_drops: row.in_discards,
                    tx_bytes: row.out_octets,
                    tx_packets: row.out_ucast_pkts + row.out_nucast_pkts,
                    tx_errors: row.out_errors,
                    tx_drops: row.out_discards,
                    speed_mbps,
                    mtu: Some(row.mtu),
                    mac_address,
                    ipv4_addresses: Vec::new(), // Would need GetIpAddrTable
                    ipv6_addresses: Vec::new(),
                });
            }

            // Free the table allocated by Windows
            FreeMibTable(table as *mut std::ffi::c_void);
        }

        Ok(interfaces)
    }
}

// macOS-specific network interface enumeration
#[cfg(target_os = "macos")]
mod macos {
    use super::*;
    use libc::{
        freeifaddrs, getifaddrs, ifaddrs, sockaddr_in, sockaddr_in6, AF_INET, AF_INET6, AF_LINK,
    };
    use std::ffi::CStr;
    use std::net::{Ipv4Addr, Ipv6Addr};
    use std::ptr;

    // if_data structure for macOS (from net/if.h)
    #[repr(C)]
    struct IfData {
        ifi_type: u8,
        ifi_typelen: u8,
        ifi_physical: u8,
        ifi_addrlen: u8,
        ifi_hdrlen: u8,
        ifi_recvquota: u8,
        ifi_xmitquota: u8,
        ifi_unused1: u8,
        ifi_mtu: u32,
        ifi_metric: u32,
        ifi_baudrate: u32,
        ifi_ipackets: u32,
        ifi_ierrors: u32,
        ifi_opackets: u32,
        ifi_oerrors: u32,
        ifi_collisions: u32,
        ifi_ibytes: u32,
        ifi_obytes: u32,
        ifi_imcasts: u32,
        ifi_omcasts: u32,
        ifi_iqdrops: u32,
        ifi_noproto: u32,
        ifi_recvtiming: u32,
        ifi_xmittiming: u32,
        ifi_lastchange: libc::timeval,
    }

    // if_data64 structure for 64-bit counters (macOS 10.7+)
    #[repr(C)]
    #[allow(dead_code)]
    struct IfData64 {
        ifi_type: u8,
        ifi_typelen: u8,
        ifi_physical: u8,
        ifi_addrlen: u8,
        ifi_hdrlen: u8,
        ifi_recvquota: u8,
        ifi_xmitquota: u8,
        ifi_unused1: u8,
        ifi_mtu: u32,
        ifi_metric: u32,
        ifi_baudrate: u64,
        ifi_ipackets: u64,
        ifi_ierrors: u64,
        ifi_opackets: u64,
        ifi_oerrors: u64,
        ifi_collisions: u64,
        ifi_ibytes: u64,
        ifi_obytes: u64,
        ifi_imcasts: u64,
        ifi_omcasts: u64,
        ifi_iqdrops: u64,
        ifi_noproto: u64,
        ifi_recvtiming: u32,
        ifi_xmittiming: u32,
        ifi_lastchange: libc::timeval,
    }

    pub fn enumerate_interfaces() -> Result<Vec<NetworkInterfaceInfo>> {
        let mut interfaces = Vec::new();
        let mut seen_interfaces: std::collections::HashMap<String, NetworkInterfaceInfo> =
            std::collections::HashMap::new();

        unsafe {
            let mut addrs: *mut ifaddrs = ptr::null_mut();
            if getifaddrs(&mut addrs) != 0 {
                return Err(IronError::System("getifaddrs failed".to_string()));
            }

            let mut current = addrs;
            while !current.is_null() {
                let ifa = &*current;

                // Get interface name
                let name = if !ifa.ifa_name.is_null() {
                    CStr::from_ptr(ifa.ifa_name).to_string_lossy().into_owned()
                } else {
                    current = ifa.ifa_next;
                    continue;
                };

                // Skip loopback
                if name == "lo0" {
                    current = ifa.ifa_next;
                    continue;
                }

                // Get or create interface entry
                let entry =
                    seen_interfaces
                        .entry(name.clone())
                        .or_insert_with(|| NetworkInterfaceInfo {
                            name: name.clone(),
                            mac_address: None,
                            ipv4_addresses: Vec::new(),
                            ipv6_addresses: Vec::new(),
                            is_up: (ifa.ifa_flags as i32 & libc::IFF_UP) != 0,
                            is_running: (ifa.ifa_flags as i32 & libc::IFF_RUNNING) != 0,
                            speed_mbps: None,
                            mtu: None,
                            rx_bytes: 0,
                            tx_bytes: 0,
                            rx_packets: 0,
                            tx_packets: 0,
                            rx_errors: 0,
                            tx_errors: 0,
                            rx_drops: 0,
                            tx_drops: 0,
                        });

                if !ifa.ifa_addr.is_null() {
                    let sa = &*ifa.ifa_addr;
                    match i32::from(sa.sa_family) {
                        AF_INET => {
                            // IPv4 address
                            let sin = &*(ifa.ifa_addr as *const sockaddr_in);
                            let ip = Ipv4Addr::from(u32::from_be(sin.sin_addr.s_addr));
                            // The field is `ipv4_addresses: Vec<String>`; an
                            // interface can hold several, and the old singular
                            // assignment would have kept only the last.
                            entry.ipv4_addresses.push(ip.to_string());
                        }
                        AF_INET6 => {
                            // IPv6 address
                            let sin6 = &*(ifa.ifa_addr as *const sockaddr_in6);
                            let ip = Ipv6Addr::from(sin6.sin6_addr.s6_addr);
                            // Skip link-local addresses
                            if !ip.is_loopback() {
                                entry.ipv6_addresses.push(ip.to_string());
                            }
                        }
                        AF_LINK => {
                            // Link layer info (MAC address and statistics)
                            // The ifa_data contains if_data structure with statistics
                            if !ifa.ifa_data.is_null() {
                                let if_data = &*(ifa.ifa_data as *const IfData);
                                entry.rx_bytes = if_data.ifi_ibytes as u64;
                                entry.tx_bytes = if_data.ifi_obytes as u64;
                                entry.rx_packets = if_data.ifi_ipackets as u64;
                                entry.tx_packets = if_data.ifi_opackets as u64;
                                entry.rx_errors = if_data.ifi_ierrors as u64;
                                entry.tx_errors = if_data.ifi_oerrors as u64;
                                entry.rx_drops = if_data.ifi_iqdrops as u64;
                                entry.mtu = Some(if_data.ifi_mtu);

                                // Speed from baudrate (bits/s to Mbps)
                                if if_data.ifi_baudrate > 0 {
                                    entry.speed_mbps =
                                        Some((if_data.ifi_baudrate / 1_000_000) as u32);
                                }
                            }

                            // Extract MAC address from sockaddr_dl
                            // sockaddr_dl is defined in net/if_dl.h
                            #[repr(C)]
                            struct SockaddrDl {
                                sdl_len: u8,
                                sdl_family: u8,
                                sdl_index: u16,
                                sdl_type: u8,
                                sdl_nlen: u8,
                                sdl_alen: u8,
                                sdl_slen: u8,
                                sdl_data: [u8; 46],
                            }

                            let sdl = &*(ifa.ifa_addr as *const SockaddrDl);
                            if sdl.sdl_alen == 6 {
                                let nlen = sdl.sdl_nlen as usize;
                                let mac = &sdl.sdl_data[nlen..nlen + 6];
                                entry.mac_address = Some(format!(
                                    "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                                    mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
                                ));
                            }
                        }
                        _ => {}
                    }
                }

                current = ifa.ifa_next;
            }

            freeifaddrs(addrs);
        }

        // Convert HashMap to Vec
        interfaces.extend(seen_interfaces.into_values());

        // Sort by name
        interfaces.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(interfaces)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_iface() -> NetworkInterfaceInfo {
        NetworkInterfaceInfo {
            name: "eth0".to_string(),
            is_up: true,
            is_running: true,
            rx_bytes: 1024 * 1024 * 100,
            rx_packets: 5000,
            rx_errors: 2,
            rx_drops: 1,
            tx_bytes: 1024 * 1024 * 50,
            tx_packets: 3000,
            tx_errors: 1,
            tx_drops: 0,
            speed_mbps: Some(1000),
            mtu: Some(1500),
            mac_address: Some("aa:bb:cc:dd:ee:ff".to_string()),
            ipv4_addresses: vec!["192.168.1.10".to_string()],
            ipv6_addresses: vec![],
        }
    }

    #[test]
    fn test_total_bytes() {
        let iface = make_test_iface();
        assert_eq!(iface.total_bytes(), iface.rx_bytes + iface.tx_bytes);
    }

    #[test]
    fn test_total_packets() {
        let iface = make_test_iface();
        assert_eq!(iface.total_packets(), 5000 + 3000);
    }

    #[test]
    fn test_total_errors() {
        let iface = make_test_iface();
        assert_eq!(iface.total_errors(), 3);
    }

    #[test]
    fn test_total_drops() {
        let iface = make_test_iface();
        assert_eq!(iface.total_drops(), 1);
    }

    #[test]
    fn test_rx_mb() {
        let iface = make_test_iface();
        assert!((iface.rx_mb() - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_tx_mb() {
        let iface = make_test_iface();
        assert!((iface.tx_mb() - 50.0).abs() < 0.01);
    }

    #[test]
    fn test_total_mb() {
        let iface = make_test_iface();
        assert!((iface.total_mb() - 150.0).abs() < 0.01);
    }

    #[test]
    fn test_is_active() {
        let mut iface = make_test_iface();
        assert!(iface.is_active());
        iface.is_up = false;
        assert!(!iface.is_active());
        iface.is_up = true;
        iface.is_running = false;
        assert!(!iface.is_active());
    }

    #[test]
    fn test_zero_stats_interface() {
        let iface = NetworkInterfaceInfo {
            name: "lo".to_string(),
            is_up: true,
            is_running: true,
            rx_bytes: 0,
            rx_packets: 0,
            rx_errors: 0,
            rx_drops: 0,
            tx_bytes: 0,
            tx_packets: 0,
            tx_errors: 0,
            tx_drops: 0,
            speed_mbps: None,
            mtu: None,
            mac_address: None,
            ipv4_addresses: vec![],
            ipv6_addresses: vec![],
        };
        assert_eq!(iface.total_bytes(), 0);
        assert_eq!(iface.total_packets(), 0);
        assert_eq!(iface.total_errors(), 0);
        assert_eq!(iface.total_drops(), 0);
        assert_eq!(iface.rx_mb(), 0.0);
        assert_eq!(iface.tx_mb(), 0.0);
    }

    /// The first sample for an interface has no rate, and says so.
    ///
    /// This asserted `rx == 0.0 && tx == 0.0`, which is what the function used
    /// to fabricate when it had nothing to difference against. A test that
    /// pins a wrong answer is worse than no test: this one passed for the
    /// entire life of the defect.
    #[test]
    fn test_bandwidth_rate_no_prev() {
        let mut monitor = NetworkMonitor {
            prev_stats: HashMap::new(),
            last_update: std::time::Instant::now(),
        };
        let iface = make_test_iface();
        assert_eq!(monitor.bandwidth_rate("eth0", &iface), None);
    }

    /// A second sample produces a rate, and it reflects the bytes between them.
    #[test]
    fn test_bandwidth_rate_uses_the_previous_sample() {
        let mut monitor = NetworkMonitor {
            prev_stats: HashMap::new(),
            last_update: std::time::Instant::now(),
        };
        let mut iface = make_test_iface();
        iface.rx_bytes = 1_000;
        iface.tx_bytes = 2_000;
        assert_eq!(monitor.bandwidth_rate("eth0", &iface), None);

        std::thread::sleep(std::time::Duration::from_millis(50));
        iface.rx_bytes = 3_000;
        iface.tx_bytes = 2_000;
        let (rx, tx) = monitor
            .bandwidth_rate("eth0", &iface)
            .expect("a second sample has something to difference against");
        // 2000 bytes over ~50ms is on the order of 40 KB/s; the exact figure
        // depends on scheduling, so this asserts the sign and the scale rather
        // than a number the test cannot control.
        assert!(rx > 0.0, "rx should reflect the 2000 new bytes, got {rx}");
        assert_eq!(tx, 0.0, "tx counters did not move, so the rate is zero");
    }
}
