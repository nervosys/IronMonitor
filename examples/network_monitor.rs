//! Network Monitor Example
//!
//! Demonstrates network interface monitoring with bandwidth tracking.
//! Shows all network interfaces with their statistics and real-time rates.

use ironmonlib::{NetworkMonitor, Result};
use std::thread;
use std::time::Duration;

fn main() -> Result<()> {
    println!("=== IronMonitor - Network Monitoring ===\n");

    // Create network monitor
    let mut monitor = match NetworkMonitor::new() {
        Ok(m) => m,
        Err(e) => {
            eprintln!("WARNING: Network monitoring not available on this platform.");
            eprintln!("Error: {}", e);
            eprintln!("\nNetwork monitoring is currently implemented for:");
            eprintln!("  - Linux: via /proc/net/dev and sysfs");
            eprintln!("  - macOS: via getif addrs (partial support)");
            eprintln!("\nWindows support is planned for future releases.");
            std::process::exit(0);
        }
    };

    println!("Network monitor initialized");
    println!("Monitoring network interfaces every 2 seconds (Ctrl+C to exit)\n");

    // Initial read to establish baseline
    let _ = monitor.interfaces()?;
    thread::sleep(Duration::from_secs(1));

    loop {
        // Get all interfaces
        let interfaces = monitor.interfaces()?;

        // Display all interfaces
        println!(
            "\n+--------------------------------------------------------------------------------+"
        );
        println!(
            "|                          NETWORK INTERFACES                                   |"
        );
        println!(
            "+----------------+---------+---------------+---------------+------------+-----------+---------+"
        );
        println!(
            "| Interface    | State | RX (MB)     | TX (MB)     | RX Pkt   | TX Pkt  | Errors|"
        );
        println!(
            "+----------------+---------+---------------+---------------+------------+-----------+---------+"
        );

        for iface in &interfaces {
            let state = if iface.is_active() {
                "  UP  "
            } else if iface.is_up {
                " UP-X "
            } else {
                " DOWN "
            };

            let errors = iface.total_errors();
            let error_marker = if errors > 0 { "!" } else { " " };

            println!(
                "| {:12} | {} | {:>11.2} | {:>11.2} | {:>8} | {:>7} | {:>5}{} |",
                iface.name,
                state,
                iface.rx_mb(),
                iface.tx_mb(),
                iface.rx_packets,
                iface.tx_packets,
                errors,
                error_marker
            );
        }

        println!(
            "+----------------+---------+---------------+---------------+------------+-----------+---------+"
        );

        // Display bandwidth rates for active interfaces
        let active_ifaces: Vec<_> = interfaces.iter().filter(|i| i.is_active()).collect();

        if !active_ifaces.is_empty() {
            println!("\n+--------------------------------------------------------------------------------+");
            println!(
                "|                        ACTIVE INTERFACE DETAILS                               |"
            );
            println!(
                "+----------------+----------------+----------------+-------------+------------+------------+"
            );
            println!(
                "| Interface    | RX Rate      | TX Rate      | Speed     | MTU      | MAC Addr |"
            );
            println!(
                "+----------------+----------------+----------------+-------------+------------+------------+"
            );

            for iface in &active_ifaces {
                // `None` on the first pass: a rate needs two samples, and
                // this is the first. The loop below samples again.
                let rate = monitor.bandwidth_rate(&iface.name, iface);

                let fmt = |r: Option<f64>| match r {
                    Some(v) => format_bandwidth(v),
                    None => "—".to_string(),
                };
                let rx_rate_str = fmt(rate.map(|(rx, _)| rx));
                let tx_rate_str = fmt(rate.map(|(_, tx)| tx));

                let speed = if let Some(s) = iface.speed_mbps {
                    format!("{} Mbps", s)
                } else {
                    "Unknown".to_string()
                };

                let mtu = if let Some(m) = iface.mtu {
                    format!("{}", m)
                } else {
                    "N/A".to_string()
                };

                let mac = if let Some(ref m) = iface.mac_address {
                    truncate_string(m, 17)
                } else {
                    "N/A".to_string()
                };

                println!(
                    "| {:12} | {:>12} | {:>12} | {:>9} | {:>8} | {:>8} |",
                    iface.name, rx_rate_str, tx_rate_str, speed, mtu, mac
                );

                // Show IP addresses if available
                if !iface.ipv4_addresses.is_empty() || !iface.ipv6_addresses.is_empty() {
                    let mut ip_display = Vec::new();
                    for ip in &iface.ipv4_addresses {
                        ip_display.push(format!("IPv4: {}", ip));
                    }
                    for ip in &iface.ipv6_addresses {
                        ip_display.push(format!("IPv6: {}", truncate_string(ip, 20)));
                    }
                    if !ip_display.is_empty() {
                        println!("|              | IPs: {:62} |", ip_display.join(", "));
                    }
                }
            }

            println!(
                "+----------------+----------------+----------------+-------------+------------+------------+"
            );
        }

        // Display summary statistics
        let total_ifaces = monitor.interface_count()?;
        let active_count = monitor.active_interface_count()?;

        let total_rx: f64 = interfaces.iter().map(|i| i.rx_mb()).sum();
        let total_tx: f64 = interfaces.iter().map(|i| i.tx_mb()).sum();
        let total_errors: u64 = interfaces.iter().map(|i| i.total_errors()).sum();
        let total_drops: u64 = interfaces.iter().map(|i| i.total_drops()).sum();

        println!("\n+--------------------------------------------------------------------------+");
        println!("|                        SUMMARY STATISTICS                               |");
        println!("+--------------------------------------------------------------------------+");
        println!("|  Total Interfaces: {:>51} |", total_ifaces);
        println!("|  Active Interfaces: {:>50} |", active_count);
        println!("|  Total RX: {:>54.2} MB |", total_rx);
        println!("|  Total TX: {:>54.2} MB |", total_tx);
        println!("|  Total Errors: {:>55} |", total_errors);
        println!("|  Total Drops: {:>56} |", total_drops);
        println!("+--------------------------------------------------------------------------+");

        // Wait before next update
        thread::sleep(Duration::from_secs(2));
    }
}

fn format_bandwidth(bytes_per_sec: f64) -> String {
    if bytes_per_sec >= 1_000_000_000.0 {
        format!("{:.2} GB/s", bytes_per_sec / 1_000_000_000.0)
    } else if bytes_per_sec >= 1_000_000.0 {
        format!("{:.2} MB/s", bytes_per_sec / 1_000_000.0)
    } else if bytes_per_sec >= 1_000.0 {
        format!("{:.2} KB/s", bytes_per_sec / 1_000.0)
    } else {
        format!("{:.0} B/s", bytes_per_sec)
    }
}

fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() > max_len {
        format!("{}...", &s[..max_len - 3])
    } else {
        s.to_string()
    }
}
