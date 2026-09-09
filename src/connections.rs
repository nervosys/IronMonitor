//! Network Connection Monitoring (netstat-like)
//!
//! This module provides cross-platform monitoring of active network connections,
//! including TCP and UDP sockets with their states, local/remote addresses, and
//! owning process information.
//!
//! # Examples
//!
//! ```no_run
//! use ironmonlib::connections::{ConnectionMonitor, Protocol};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let monitor = ConnectionMonitor::new()?;
//!
//! // Get all TCP connections
//! let tcp_conns = monitor.tcp_connections()?;
//! for conn in tcp_conns {
//!     println!("{} -> {} ({})",
//!         conn.local_address,
//!         conn.remote_address.unwrap_or("*".to_string()),
//!         conn.state
//!     );
//! }
//! # Ok(())
//! # }
//! ```

use serde::{Deserialize, Serialize};
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// Network connection information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionInfo {
    /// Protocol (TCP, UDP, TCP6, UDP6)
    pub protocol: Protocol,
    /// Local address (IP:port)
    pub local_address: String,
    /// Local IP only
    pub local_ip: IpAddr,
    /// Local port
    pub local_port: u16,
    /// Remote address (IP:port), None for UDP listeners
    pub remote_address: Option<String>,
    /// Remote IP only
    pub remote_ip: Option<IpAddr>,
    /// Remote port
    pub remote_port: Option<u16>,
    /// Connection state (for TCP)
    pub state: ConnectionState,
    /// Owning process ID
    pub pid: Option<u32>,
    /// Owning process name (if available)
    pub process_name: Option<String>,
}

/// Network protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Protocol {
    Tcp,
    Tcp6,
    Udp,
    Udp6,
}

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Protocol::Tcp => write!(f, "TCP"),
            Protocol::Tcp6 => write!(f, "TCP6"),
            Protocol::Udp => write!(f, "UDP"),
            Protocol::Udp6 => write!(f, "UDP6"),
        }
    }
}

/// TCP connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ConnectionState {
    #[default]
    Unknown,
    Closed,
    Listen,
    SynSent,
    SynReceived,
    Established,
    FinWait1,
    FinWait2,
    CloseWait,
    Closing,
    LastAck,
    TimeWait,
    DeleteTcb,
    // UDP has no state
    Stateless,
}

impl fmt::Display for ConnectionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConnectionState::Unknown => write!(f, "UNKNOWN"),
            ConnectionState::Closed => write!(f, "CLOSED"),
            ConnectionState::Listen => write!(f, "LISTEN"),
            ConnectionState::SynSent => write!(f, "SYN_SENT"),
            ConnectionState::SynReceived => write!(f, "SYN_RECV"),
            ConnectionState::Established => write!(f, "ESTABLISHED"),
            ConnectionState::FinWait1 => write!(f, "FIN_WAIT1"),
            ConnectionState::FinWait2 => write!(f, "FIN_WAIT2"),
            ConnectionState::CloseWait => write!(f, "CLOSE_WAIT"),
            ConnectionState::Closing => write!(f, "CLOSING"),
            ConnectionState::LastAck => write!(f, "LAST_ACK"),
            ConnectionState::TimeWait => write!(f, "TIME_WAIT"),
            ConnectionState::DeleteTcb => write!(f, "DELETE_TCB"),
            ConnectionState::Stateless => write!(f, "-"),
        }
    }
}

/// Connection monitor for network sockets
pub struct ConnectionMonitor {
    /// Cache of process names by PID
    #[allow(dead_code)]
    process_cache: std::collections::HashMap<u32, String>,
}

impl ConnectionMonitor {
    /// Create a new connection monitor
    pub fn new() -> Result<Self, Error> {
        Ok(Self {
            process_cache: std::collections::HashMap::new(),
        })
    }

    /// Get all TCP (IPv4) connections
    pub fn tcp_connections(&self) -> Result<Vec<ConnectionInfo>, Error> {
        #[cfg(target_os = "windows")]
        return self.windows_tcp_connections();

        #[cfg(target_os = "linux")]
        return self.linux_tcp_connections();

        #[cfg(target_os = "macos")]
        return self.macos_tcp_connections();

        #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
        Err(Error::NotSupported("Platform not supported".into()))
    }

    /// Get all TCP6 (IPv6) connections
    pub fn tcp6_connections(&self) -> Result<Vec<ConnectionInfo>, Error> {
        #[cfg(target_os = "windows")]
        return self.windows_tcp6_connections();

        #[cfg(target_os = "linux")]
        return self.linux_tcp6_connections();

        #[cfg(target_os = "macos")]
        return self.macos_tcp6_connections();

        #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
        Err(Error::NotSupported("Platform not supported".into()))
    }

    /// Get all UDP (IPv4) endpoints
    pub fn udp_endpoints(&self) -> Result<Vec<ConnectionInfo>, Error> {
        #[cfg(target_os = "windows")]
        return self.windows_udp_endpoints();

        #[cfg(target_os = "linux")]
        return self.linux_udp_endpoints();

        #[cfg(target_os = "macos")]
        return self.macos_udp_endpoints();

        #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
        Err(Error::NotSupported("Platform not supported".into()))
    }

    /// Get all UDP6 (IPv6) endpoints
    pub fn udp6_endpoints(&self) -> Result<Vec<ConnectionInfo>, Error> {
        #[cfg(target_os = "windows")]
        return self.windows_udp6_endpoints();

        #[cfg(target_os = "linux")]
        return self.linux_udp6_endpoints();

        #[cfg(target_os = "macos")]
        return self.macos_udp6_endpoints();

        #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
        Err(Error::NotSupported("Platform not supported".into()))
    }

    /// Get all connections (TCP + UDP, IPv4 + IPv6)
    pub fn all_connections(&self) -> Result<Vec<ConnectionInfo>, Error> {
        let mut all = Vec::new();

        if let Ok(tcp) = self.tcp_connections() {
            all.extend(tcp);
        }
        if let Ok(tcp6) = self.tcp6_connections() {
            all.extend(tcp6);
        }
        if let Ok(udp) = self.udp_endpoints() {
            all.extend(udp);
        }
        if let Ok(udp6) = self.udp6_endpoints() {
            all.extend(udp6);
        }

        Ok(all)
    }

    /// Get only established TCP connections
    pub fn established_connections(&self) -> Result<Vec<ConnectionInfo>, Error> {
        let all = self.all_connections()?;
        Ok(all
            .into_iter()
            .filter(|c| c.state == ConnectionState::Established)
            .collect())
    }

    /// Get only listening sockets
    pub fn listening_sockets(&self) -> Result<Vec<ConnectionInfo>, Error> {
        let all = self.all_connections()?;
        Ok(all
            .into_iter()
            .filter(|c| c.state == ConnectionState::Listen || c.state == ConnectionState::Stateless)
            .collect())
    }
}

// Windows implementation
#[cfg(target_os = "windows")]
impl ConnectionMonitor {
    fn windows_tcp_connections(&self) -> Result<Vec<ConnectionInfo>, Error> {
        use windows::Win32::NetworkManagement::IpHelper::{
            GetExtendedTcpTable, MIB_TCPTABLE_OWNER_PID, TCP_TABLE_OWNER_PID_ALL,
        };
        use windows::Win32::Networking::WinSock::AF_INET;

        let mut connections = Vec::new();
        let mut size: u32 = 0;

        // First call to get required size
        unsafe {
            let _ = GetExtendedTcpTable(
                None,
                &mut size,
                false,
                AF_INET.0 as u32,
                TCP_TABLE_OWNER_PID_ALL,
                0,
            );
        }

        if size == 0 {
            return Ok(connections);
        }

        let mut buffer = vec![0u8; size as usize];
        let result = unsafe {
            GetExtendedTcpTable(
                Some(buffer.as_mut_ptr() as *mut _),
                &mut size,
                false,
                AF_INET.0 as u32,
                TCP_TABLE_OWNER_PID_ALL,
                0,
            )
        };

        if result != 0 {
            return Err(Error::SystemError(format!(
                "GetExtendedTcpTable failed: {}",
                result
            )));
        }

        let table = unsafe { &*(buffer.as_ptr() as *const MIB_TCPTABLE_OWNER_PID) };
        let rows = table.dwNumEntries as usize;

        for i in 0..rows {
            let row = unsafe {
                let row_ptr = table.table.as_ptr().add(i);
                &*row_ptr
            };

            let local_ip = Ipv4Addr::from(u32::from_be(row.dwLocalAddr));
            let local_port = u16::from_be((row.dwLocalPort & 0xFFFF) as u16);
            let remote_ip = Ipv4Addr::from(u32::from_be(row.dwRemoteAddr));
            let remote_port = u16::from_be((row.dwRemotePort & 0xFFFF) as u16);
            let state = self.windows_tcp_state(row.dwState);
            let pid = row.dwOwningPid;

            connections.push(ConnectionInfo {
                protocol: Protocol::Tcp,
                local_address: format!("{}:{}", local_ip, local_port),
                local_ip: IpAddr::V4(local_ip),
                local_port,
                remote_address: if state != ConnectionState::Listen {
                    Some(format!("{}:{}", remote_ip, remote_port))
                } else {
                    None
                },
                remote_ip: if state != ConnectionState::Listen {
                    Some(IpAddr::V4(remote_ip))
                } else {
                    None
                },
                remote_port: if state != ConnectionState::Listen {
                    Some(remote_port)
                } else {
                    None
                },
                state,
                pid: Some(pid),
                process_name: self.get_process_name(pid),
            });
        }

        Ok(connections)
    }

    fn windows_tcp6_connections(&self) -> Result<Vec<ConnectionInfo>, Error> {
        use windows::Win32::NetworkManagement::IpHelper::{
            GetExtendedTcpTable, MIB_TCP6TABLE_OWNER_PID, TCP_TABLE_OWNER_PID_ALL,
        };
        use windows::Win32::Networking::WinSock::AF_INET6;

        let mut connections = Vec::new();
        let mut size: u32 = 0;

        unsafe {
            let _ = GetExtendedTcpTable(
                None,
                &mut size,
                false,
                AF_INET6.0 as u32,
                TCP_TABLE_OWNER_PID_ALL,
                0,
            );
        }

        if size == 0 {
            return Ok(connections);
        }

        let mut buffer = vec![0u8; size as usize];
        let result = unsafe {
            GetExtendedTcpTable(
                Some(buffer.as_mut_ptr() as *mut _),
                &mut size,
                false,
                AF_INET6.0 as u32,
                TCP_TABLE_OWNER_PID_ALL,
                0,
            )
        };

        if result != 0 {
            return Err(Error::SystemError(format!(
                "GetExtendedTcpTable IPv6 failed: {}",
                result
            )));
        }

        let table = unsafe { &*(buffer.as_ptr() as *const MIB_TCP6TABLE_OWNER_PID) };
        let rows = table.dwNumEntries as usize;

        for i in 0..rows {
            let row = unsafe {
                let row_ptr = table.table.as_ptr().add(i);
                &*row_ptr
            };

            let local_ip = Ipv6Addr::from(row.ucLocalAddr);
            let local_port = u16::from_be((row.dwLocalPort & 0xFFFF) as u16);
            let remote_ip = Ipv6Addr::from(row.ucRemoteAddr);
            let remote_port = u16::from_be((row.dwRemotePort & 0xFFFF) as u16);
            let state = self.windows_tcp_state(row.dwState);
            let pid = row.dwOwningPid;

            connections.push(ConnectionInfo {
                protocol: Protocol::Tcp6,
                local_address: format!("[{}]:{}", local_ip, local_port),
                local_ip: IpAddr::V6(local_ip),
                local_port,
                remote_address: if state != ConnectionState::Listen {
                    Some(format!("[{}]:{}", remote_ip, remote_port))
                } else {
                    None
                },
                remote_ip: if state != ConnectionState::Listen {
                    Some(IpAddr::V6(remote_ip))
                } else {
                    None
                },
                remote_port: if state != ConnectionState::Listen {
                    Some(remote_port)
                } else {
                    None
                },
                state,
                pid: Some(pid),
                process_name: self.get_process_name(pid),
            });
        }

        Ok(connections)
    }

    fn windows_udp_endpoints(&self) -> Result<Vec<ConnectionInfo>, Error> {
        use windows::Win32::NetworkManagement::IpHelper::{
            GetExtendedUdpTable, MIB_UDPTABLE_OWNER_PID, UDP_TABLE_OWNER_PID,
        };
        use windows::Win32::Networking::WinSock::AF_INET;

        let mut connections = Vec::new();
        let mut size: u32 = 0;

        unsafe {
            let _ = GetExtendedUdpTable(
                None,
                &mut size,
                false,
                AF_INET.0 as u32,
                UDP_TABLE_OWNER_PID,
                0,
            );
        }

        if size == 0 {
            return Ok(connections);
        }

        let mut buffer = vec![0u8; size as usize];
        let result = unsafe {
            GetExtendedUdpTable(
                Some(buffer.as_mut_ptr() as *mut _),
                &mut size,
                false,
                AF_INET.0 as u32,
                UDP_TABLE_OWNER_PID,
                0,
            )
        };

        if result != 0 {
            return Err(Error::SystemError(format!(
                "GetExtendedUdpTable failed: {}",
                result
            )));
        }

        let table = unsafe { &*(buffer.as_ptr() as *const MIB_UDPTABLE_OWNER_PID) };
        let rows = table.dwNumEntries as usize;

        for i in 0..rows {
            let row = unsafe {
                let row_ptr = table.table.as_ptr().add(i);
                &*row_ptr
            };

            let local_ip = Ipv4Addr::from(u32::from_be(row.dwLocalAddr));
            let local_port = u16::from_be((row.dwLocalPort & 0xFFFF) as u16);
            let pid = row.dwOwningPid;

            connections.push(ConnectionInfo {
                protocol: Protocol::Udp,
                local_address: format!("{}:{}", local_ip, local_port),
                local_ip: IpAddr::V4(local_ip),
                local_port,
                remote_address: None,
                remote_ip: None,
                remote_port: None,
                state: ConnectionState::Stateless,
                pid: Some(pid),
                process_name: self.get_process_name(pid),
            });
        }

        Ok(connections)
    }

    fn windows_udp6_endpoints(&self) -> Result<Vec<ConnectionInfo>, Error> {
        use windows::Win32::NetworkManagement::IpHelper::{
            GetExtendedUdpTable, MIB_UDP6TABLE_OWNER_PID, UDP_TABLE_OWNER_PID,
        };
        use windows::Win32::Networking::WinSock::AF_INET6;

        let mut connections = Vec::new();
        let mut size: u32 = 0;

        unsafe {
            let _ = GetExtendedUdpTable(
                None,
                &mut size,
                false,
                AF_INET6.0 as u32,
                UDP_TABLE_OWNER_PID,
                0,
            );
        }

        if size == 0 {
            return Ok(connections);
        }

        let mut buffer = vec![0u8; size as usize];
        let result = unsafe {
            GetExtendedUdpTable(
                Some(buffer.as_mut_ptr() as *mut _),
                &mut size,
                false,
                AF_INET6.0 as u32,
                UDP_TABLE_OWNER_PID,
                0,
            )
        };

        if result != 0 {
            return Err(Error::SystemError(format!(
                "GetExtendedUdpTable IPv6 failed: {}",
                result
            )));
        }

        let table = unsafe { &*(buffer.as_ptr() as *const MIB_UDP6TABLE_OWNER_PID) };
        let rows = table.dwNumEntries as usize;

        for i in 0..rows {
            let row = unsafe {
                let row_ptr = table.table.as_ptr().add(i);
                &*row_ptr
            };

            let local_ip = Ipv6Addr::from(row.ucLocalAddr);
            let local_port = u16::from_be((row.dwLocalPort & 0xFFFF) as u16);
            let pid = row.dwOwningPid;

            connections.push(ConnectionInfo {
                protocol: Protocol::Udp6,
                local_address: format!("[{}]:{}", local_ip, local_port),
                local_ip: IpAddr::V6(local_ip),
                local_port,
                remote_address: None,
                remote_ip: None,
                remote_port: None,
                state: ConnectionState::Stateless,
                pid: Some(pid),
                process_name: self.get_process_name(pid),
            });
        }

        Ok(connections)
    }

    fn windows_tcp_state(&self, state: u32) -> ConnectionState {
        match state {
            1 => ConnectionState::Closed,
            2 => ConnectionState::Listen,
            3 => ConnectionState::SynSent,
            4 => ConnectionState::SynReceived,
            5 => ConnectionState::Established,
            6 => ConnectionState::FinWait1,
            7 => ConnectionState::FinWait2,
            8 => ConnectionState::CloseWait,
            9 => ConnectionState::Closing,
            10 => ConnectionState::LastAck,
            11 => ConnectionState::TimeWait,
            12 => ConnectionState::DeleteTcb,
            _ => ConnectionState::Unknown,
        }
    }

    fn get_process_name(&self, pid: u32) -> Option<String> {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::ProcessStatus::GetModuleBaseNameW;
        use windows::Win32::System::Threading::{
            OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
        };

        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid);

            if let Ok(handle) = handle {
                let mut name = [0u16; 260];
                let len = GetModuleBaseNameW(handle, None, &mut name);
                let _ = CloseHandle(handle);

                if len > 0 {
                    return Some(String::from_utf16_lossy(&name[..len as usize]));
                }
            }
        }
        None
    }
}

// Linux implementation
#[cfg(target_os = "linux")]
impl ConnectionMonitor {
    fn linux_tcp_connections(&self) -> Result<Vec<ConnectionInfo>, Error> {
        self.parse_proc_net("/proc/net/tcp", Protocol::Tcp)
    }

    fn linux_tcp6_connections(&self) -> Result<Vec<ConnectionInfo>, Error> {
        self.parse_proc_net("/proc/net/tcp6", Protocol::Tcp6)
    }

    fn linux_udp_endpoints(&self) -> Result<Vec<ConnectionInfo>, Error> {
        self.parse_proc_net("/proc/net/udp", Protocol::Udp)
    }

    fn linux_udp6_endpoints(&self) -> Result<Vec<ConnectionInfo>, Error> {
        self.parse_proc_net("/proc/net/udp6", Protocol::Udp6)
    }

    fn parse_proc_net(&self, path: &str, protocol: Protocol) -> Result<Vec<ConnectionInfo>, Error> {
        use std::fs;
        use std::io::{BufRead, BufReader};

        let file = fs::File::open(path)
            .map_err(|e| Error::IoError(format!("Failed to open {}: {}", path, e)))?;
        let reader = BufReader::new(file);
        let mut connections = Vec::new();

        for (i, line) in reader.lines().enumerate() {
            if i == 0 {
                continue; // Skip header
            }
            let line = line.map_err(|e| Error::IoError(e.to_string()))?;
            if let Some(conn) = self.parse_proc_net_line(&line, protocol) {
                connections.push(conn);
            }
        }

        Ok(connections)
    }

    fn parse_proc_net_line(&self, line: &str, protocol: Protocol) -> Option<ConnectionInfo> {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 10 {
            return None;
        }

        let local = parts[1];
        let remote = parts[2];
        let state_hex = parts[3];
        let _uid = parts[7].parse::<u32>().ok();
        let inode = parts[9];

        let is_ipv6 = matches!(protocol, Protocol::Tcp6 | Protocol::Udp6);

        let (local_ip, local_port) = self.parse_address(local, is_ipv6)?;
        let (remote_ip, remote_port) = self.parse_address(remote, is_ipv6)?;

        let state = if matches!(protocol, Protocol::Tcp | Protocol::Tcp6) {
            self.linux_tcp_state(u32::from_str_radix(state_hex, 16).unwrap_or(0))
        } else {
            ConnectionState::Stateless
        };

        let pid = self.find_pid_by_inode(inode);
        let process_name = pid.and_then(|p| self.get_process_name_linux(p));

        Some(ConnectionInfo {
            protocol,
            local_address: format!("{}:{}", local_ip, local_port),
            local_ip,
            local_port,
            remote_address: if state != ConnectionState::Listen && remote_port != 0 {
                Some(format!("{}:{}", remote_ip, remote_port))
            } else {
                None
            },
            remote_ip: if state != ConnectionState::Listen && remote_port != 0 {
                Some(remote_ip)
            } else {
                None
            },
            remote_port: if state != ConnectionState::Listen && remote_port != 0 {
                Some(remote_port)
            } else {
                None
            },
            state,
            pid,
            process_name,
        })
    }

    fn parse_address(&self, addr: &str, is_ipv6: bool) -> Option<(IpAddr, u16)> {
        let parts: Vec<&str> = addr.split(':').collect();
        if parts.len() != 2 {
            return None;
        }

        let port = u16::from_str_radix(parts[1], 16).ok()?;

        let ip = if is_ipv6 {
            let bytes: Vec<u8> = (0..32)
                .step_by(2)
                .filter_map(|i| u8::from_str_radix(&parts[0][i..i + 2], 16).ok())
                .collect();
            if bytes.len() != 16 {
                return None;
            }
            let mut arr = [0u8; 16];
            arr.copy_from_slice(&bytes);
            IpAddr::V6(Ipv6Addr::from(arr))
        } else {
            let ip_num = u32::from_str_radix(parts[0], 16).ok()?;
            IpAddr::V4(Ipv4Addr::from(ip_num.to_be()))
        };

        Some((ip, port))
    }

    fn linux_tcp_state(&self, state: u32) -> ConnectionState {
        match state {
            0x01 => ConnectionState::Established,
            0x02 => ConnectionState::SynSent,
            0x03 => ConnectionState::SynReceived,
            0x04 => ConnectionState::FinWait1,
            0x05 => ConnectionState::FinWait2,
            0x06 => ConnectionState::TimeWait,
            0x07 => ConnectionState::Closed,
            0x08 => ConnectionState::CloseWait,
            0x09 => ConnectionState::LastAck,
            0x0A => ConnectionState::Listen,
            0x0B => ConnectionState::Closing,
            _ => ConnectionState::Unknown,
        }
    }

    fn find_pid_by_inode(&self, inode: &str) -> Option<u32> {
        use std::fs;

        let proc_dir = match fs::read_dir("/proc") {
            Ok(d) => d,
            Err(_) => return None,
        };

        for entry in proc_dir.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name() {
                if let Some(name_str) = name.to_str() {
                    if let Ok(pid) = name_str.parse::<u32>() {
                        let fd_path = path.join("fd");
                        if let Ok(fds) = fs::read_dir(&fd_path) {
                            for fd in fds.flatten() {
                                if let Ok(link) = fs::read_link(fd.path()) {
                                    if let Some(link_str) = link.to_str() {
                                        if link_str.contains(&format!("socket:[{}]", inode)) {
                                            return Some(pid);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        None
    }

    fn get_process_name_linux(&self, pid: u32) -> Option<String> {
        use std::fs;

        let comm_path = format!("/proc/{}/comm", pid);
        fs::read_to_string(&comm_path)
            .ok()
            .map(|s| s.trim().to_string())
    }
}

// macOS implementation using netstat
#[cfg(target_os = "macos")]
impl ConnectionMonitor {
    fn macos_tcp_connections(&self) -> Result<Vec<ConnectionInfo>, Error> {
        self.parse_macos_netstat(Protocol::Tcp, "tcp4")
    }

    fn macos_tcp6_connections(&self) -> Result<Vec<ConnectionInfo>, Error> {
        self.parse_macos_netstat(Protocol::Tcp6, "tcp6")
    }

    fn macos_udp_endpoints(&self) -> Result<Vec<ConnectionInfo>, Error> {
        self.parse_macos_netstat(Protocol::Udp, "udp4")
    }

    fn macos_udp6_endpoints(&self) -> Result<Vec<ConnectionInfo>, Error> {
        self.parse_macos_netstat(Protocol::Udp6, "udp6")
    }

    fn parse_macos_netstat(
        &self,
        protocol: Protocol,
        proto_flag: &str,
    ) -> Result<Vec<ConnectionInfo>, Error> {
        use std::process::Command;

        let output = Command::new("netstat")
            .args(["-an", "-p", proto_flag])
            .output()
            .map_err(|e| Error::SystemError(format!("Failed to run netstat: {}", e)))?;

        if !output.status.success() {
            return Err(Error::SystemError("netstat command failed".into()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut connections = Vec::new();

        for line in stdout.lines().skip(2) {
            // Skip header lines
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 4 {
                continue;
            }

            // macOS netstat format: Proto Recv-Q Send-Q Local Address Foreign Address (State)
            let local_addr_str = parts[3];
            let remote_addr_str = if parts.len() > 4 {
                Some(parts[4])
            } else {
                None
            };
            let state_str = if parts.len() > 5 {
                Some(parts[5])
            } else {
                None
            };

            let (local_ip, local_port) = match Self::parse_macos_address(local_addr_str, &protocol)
            {
                Some(v) => v,
                None => continue,
            };

            let (remote_ip, remote_port) = if let Some(addr) = remote_addr_str {
                match Self::parse_macos_address(addr, &protocol) {
                    Some(v) => (Some(v.0), Some(v.1)),
                    None => (None, None),
                }
            } else {
                (None, None)
            };

            let state = if let Some(s) = state_str {
                Self::parse_macos_state(s)
            } else if matches!(protocol, Protocol::Udp | Protocol::Udp6) {
                ConnectionState::Stateless
            } else {
                ConnectionState::Unknown
            };

            connections.push(ConnectionInfo {
                protocol,
                local_address: local_addr_str.to_string(),
                local_ip,
                local_port,
                remote_address: remote_addr_str.map(|s| s.to_string()),
                remote_ip,
                remote_port,
                state,
                pid: None, // netstat -an doesn't show PIDs; would need lsof -i
                process_name: None,
            });
        }

        Ok(connections)
    }

    fn parse_macos_address(addr: &str, protocol: &Protocol) -> Option<(IpAddr, u16)> {
        // macOS format: IP.port or [IPv6]:port or *.port
        if let Some(last_dot) = addr.rfind('.') {
            let ip_part = &addr[..last_dot];
            let port_str = &addr[last_dot + 1..];
            let port: u16 = match port_str.parse() {
                Ok(p) => p,
                Err(_) => {
                    // Could be a service name like "https"
                    if port_str == "*" {
                        0
                    } else {
                        return None;
                    }
                }
            };

            let ip = if ip_part == "*" {
                match protocol {
                    Protocol::Tcp6 | Protocol::Udp6 => IpAddr::V6(Ipv6Addr::UNSPECIFIED),
                    _ => IpAddr::V4(Ipv4Addr::UNSPECIFIED),
                }
            } else if let Ok(v4) = ip_part.parse::<Ipv4Addr>() {
                IpAddr::V4(v4)
            } else if let Ok(v6) = ip_part.parse::<Ipv6Addr>() {
                IpAddr::V6(v6)
            } else {
                // For IPv6 on macOS like "fe80::1%lo0.port" or bracket form
                let cleaned = ip_part.trim_matches(|c| c == '[' || c == ']');
                // Strip zone ID (e.g., %lo0)
                let cleaned = if let Some(pct) = cleaned.find('%') {
                    &cleaned[..pct]
                } else {
                    cleaned
                };
                match cleaned.parse::<Ipv6Addr>() {
                    Ok(v6) => IpAddr::V6(v6),
                    Err(_) => return None,
                }
            };

            Some((ip, port))
        } else {
            None
        }
    }

    fn parse_macos_state(state: &str) -> ConnectionState {
        match state {
            "ESTABLISHED" => ConnectionState::Established,
            "LISTEN" => ConnectionState::Listen,
            "SYN_SENT" => ConnectionState::SynSent,
            "SYN_RECEIVED" | "SYN_RECV" => ConnectionState::SynReceived,
            "FIN_WAIT_1" | "FIN_WAIT1" => ConnectionState::FinWait1,
            "FIN_WAIT_2" | "FIN_WAIT2" => ConnectionState::FinWait2,
            "CLOSE_WAIT" => ConnectionState::CloseWait,
            "CLOSING" => ConnectionState::Closing,
            "LAST_ACK" => ConnectionState::LastAck,
            "TIME_WAIT" => ConnectionState::TimeWait,
            "CLOSED" => ConnectionState::Closed,
            _ => ConnectionState::Unknown,
        }
    }

    fn get_process_name_macos(&self, _pid: u32) -> Option<String> {
        // Would need libproc or ps command
        None
    }
}

/// Connection monitoring errors
#[derive(Debug)]
pub enum Error {
    NotSupported(String),
    SystemError(String),
    IoError(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NotSupported(msg) => write!(f, "Not supported: {}", msg),
            Error::SystemError(msg) => write!(f, "System error: {}", msg),
            Error::IoError(msg) => write!(f, "IO error: {}", msg),
        }
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn test_protocol_display() {
        assert_eq!(Protocol::Tcp.to_string(), "TCP");
        assert_eq!(Protocol::Tcp6.to_string(), "TCP6");
        assert_eq!(Protocol::Udp.to_string(), "UDP");
        assert_eq!(Protocol::Udp6.to_string(), "UDP6");
    }

    #[test]
    fn test_connection_state_default() {
        assert_eq!(ConnectionState::default(), ConnectionState::Unknown);
    }

    #[test]
    fn test_connection_state_display_all_variants() {
        let states = [
            ConnectionState::Unknown,
            ConnectionState::Closed,
            ConnectionState::Listen,
            ConnectionState::SynSent,
            ConnectionState::SynReceived,
            ConnectionState::Established,
            ConnectionState::FinWait1,
            ConnectionState::FinWait2,
            ConnectionState::CloseWait,
            ConnectionState::Closing,
            ConnectionState::LastAck,
            ConnectionState::TimeWait,
            ConnectionState::DeleteTcb,
            ConnectionState::Stateless,
        ];
        for state in &states {
            let s = state.to_string();
            assert!(!s.is_empty(), "Display for {:?} should not be empty", state);
        }
    }

    #[test]
    fn test_protocol_serde_roundtrip() {
        for proto in &[Protocol::Tcp, Protocol::Tcp6, Protocol::Udp, Protocol::Udp6] {
            let json = serde_json::to_string(proto).unwrap();
            let parsed: Protocol = serde_json::from_str(&json).unwrap();
            assert_eq!(&parsed, proto);
        }
    }

    #[test]
    fn test_connection_info_serde_roundtrip() {
        let info = ConnectionInfo {
            protocol: Protocol::Tcp,
            local_address: "127.0.0.1:8080".into(),
            local_ip: IpAddr::V4(Ipv4Addr::LOCALHOST),
            local_port: 8080,
            remote_address: Some("10.0.0.1:443".into()),
            remote_ip: Some(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))),
            remote_port: Some(443),
            state: ConnectionState::Established,
            pid: Some(1234),
            process_name: Some("curl".into()),
        };
        let json = serde_json::to_string(&info).unwrap();
        let parsed: ConnectionInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.protocol, Protocol::Tcp);
        assert_eq!(parsed.local_port, 8080);
        assert_eq!(parsed.state, ConnectionState::Established);
        assert_eq!(parsed.pid, Some(1234));
    }

    #[test]
    fn test_connection_monitor_new() {
        let monitor = ConnectionMonitor::new();
        assert!(monitor.is_ok());
    }
}
