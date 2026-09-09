//! Network Diagnostic Tools Example
//!
//! Demonstrates nmap-style port scanning, ping, traceroute, and other
//! network diagnostic utilities.
//!
//! Run with: cargo run --example network_tools

use ironmonlib::{
    check_connectivity, check_port, common_ports, dns_lookup, get_service_name, latency_test, ping,
    reverse_dns, scan_ports, traceroute, PingResult, PortStatus, TracerouteResult,
};
use std::time::Duration;

fn main() {
    println!("╔════════════════════════════════════════════════════════════════════╗");
    println!("║           IronMonitor - Network Diagnostic Tools              ║");
    println!("║     (nmap • traceroute • ping • netcat style utilities)           ║");
    println!("╚════════════════════════════════════════════════════════════════════╝");
    println!();

    // Target for demonstrations
    let target = "8.8.8.8"; // Google's public DNS
    let target_host = "google.com";

    // 1. Ping test
    demo_ping(target);

    // 2. DNS lookup
    demo_dns(target_host);

    // 3. Connectivity check
    demo_connectivity();

    // 4. Traceroute
    demo_traceroute(target_host);

    // 5. Port scanning
    demo_port_scan(target);

    // 6. Well-known service ports
    demo_service_names();

    // 7. Latency test
    demo_latency(target);

    println!("\n✓ All network diagnostic demos complete!");
}

fn demo_ping(target: &str) {
    println!("┌─────────────────────────────────────────────────────────────────────┐");
    println!("│  1. PING - Connectivity Test                                        │");
    println!("└─────────────────────────────────────────────────────────────────────┘");

    println!("Pinging {} with 4 ICMP echo requests...", target);

    match ping(target, 4) {
        Ok(result) => {
            print_ping_result(&result);
        }
        Err(e) => {
            println!("  ✗ Ping failed: {}", e);
        }
    }
    println!();
}

fn print_ping_result(result: &PingResult) {
    println!();
    if let Some(ip) = &result.ip_address {
        println!("  Pinging {} [{}]", result.host, ip);
    } else {
        println!("  Pinging {}", result.host);
    }

    println!();
    println!("  Ping statistics for {}:", result.host);
    println!(
        "    Packets: Sent = {}, Received = {}, Lost = {} ({:.0}% loss)",
        result.packets_sent,
        result.packets_received,
        result.packets_lost,
        result.packet_loss_percent
    );

    if result.is_reachable {
        println!("  Approximate round trip times:");
        println!(
            "    Minimum = {:.2}ms, Maximum = {:.2}ms, Average = {:.2}ms",
            result.rtt_min_ms, result.rtt_max_ms, result.rtt_avg_ms
        );
        if let Some(stddev) = result.rtt_stddev_ms {
            println!("    StdDev = {:.2}ms", stddev);
        }
        println!();
        println!("  Status: ✓ Host is REACHABLE");
    } else {
        println!();
        println!("  Status: ✗ Host is UNREACHABLE");
    }
}

fn demo_dns(hostname: &str) {
    println!("┌─────────────────────────────────────────────────────────────────────┐");
    println!("│  2. DNS LOOKUP - Name Resolution                                    │");
    println!("└─────────────────────────────────────────────────────────────────────┘");

    println!("  Looking up '{}'...", hostname);

    match dns_lookup(hostname) {
        Ok(addrs) => {
            println!("  Resolved addresses:");
            for addr in addrs {
                println!("    → {}", addr);
            }
        }
        Err(e) => {
            println!("  ✗ DNS lookup failed: {}", e);
        }
    }

    // Reverse DNS
    println!();
    println!("  Reverse DNS for 8.8.8.8:");
    match reverse_dns("8.8.8.8") {
        Ok(Some(hostname)) => {
            println!("    → {}", hostname);
        }
        Ok(None) => {
            println!("    → (no PTR record found)");
        }
        Err(e) => {
            println!("    ✗ Reverse DNS failed: {}", e);
        }
    }
    println!();
}

fn demo_connectivity() {
    println!("┌─────────────────────────────────────────────────────────────────────┐");
    println!("│  3. CONNECTIVITY CHECK - Multi-host Reachability                    │");
    println!("└─────────────────────────────────────────────────────────────────────┘");

    let hosts = ["8.8.8.8", "1.1.1.1", "208.67.222.222", "192.0.2.1"]; // Last one is TEST-NET, unreachable

    println!("  Checking connectivity to multiple hosts...");
    println!();

    let results = check_connectivity(&hosts);

    for (host, reachable) in &results {
        let status = if *reachable {
            "✓ Reachable"
        } else {
            "✗ Unreachable"
        };
        println!("    {:20} {}", host, status);
    }
    println!();
}

fn demo_traceroute(target: &str) {
    println!("┌─────────────────────────────────────────────────────────────────────┐");
    println!("│  4. TRACEROUTE - Path Tracing                                       │");
    println!("└─────────────────────────────────────────────────────────────────────┘");

    println!("  Tracing route to {} (max 15 hops)...", target);
    println!();

    match traceroute(target, 15) {
        Ok(result) => {
            print_traceroute_result(&result);
        }
        Err(e) => {
            println!("  ✗ Traceroute failed: {}", e);
            println!("    (May require administrator/root privileges)");
        }
    }
    println!();
}

fn print_traceroute_result(result: &TracerouteResult) {
    println!(
        "  Tracing route to {} [{}]",
        result.target,
        result.target_ip.as_deref().unwrap_or("unknown")
    );
    println!("  over a maximum of {} hops:\n", result.total_hops);
    println!("  {:>3}  {:>15}  {:>12}  Hostname", "Hop", "Address", "RTT");
    println!("  {:─>3}  {:─>15}  {:─>12}  {:─>20}", "", "", "", "");

    for hop in &result.hops {
        let addr = hop.address.as_deref().unwrap_or("*");
        let rtt = hop
            .rtt_ms
            .map(|r| format!("{:.2} ms", r))
            .unwrap_or_else(|| "*".to_string());
        let hostname = hop.hostname.as_deref().unwrap_or("");

        println!("  {:>3}  {:>15}  {:>12}  {}", hop.ttl, addr, rtt, hostname);
    }

    if result.destination_reached {
        println!("\n  ✓ Destination reached in {} hops", result.hops.len());
    } else {
        println!(
            "\n  ✗ Destination not reached within {} hops",
            result.total_hops
        );
    }
}

fn demo_port_scan(target: &str) {
    println!("┌─────────────────────────────────────────────────────────────────────┐");
    println!("│  5. PORT SCAN - nmap-style TCP Connect Scan                         │");
    println!("└─────────────────────────────────────────────────────────────────────┘");

    // Scan common ports
    let ports_to_scan = [
        21, 22, 23, 25, 53, 80, 110, 143, 443, 993, 995, 3306, 3389, 5432, 8080,
    ];

    println!(
        "  Scanning {} for {} common ports...",
        target,
        ports_to_scan.len()
    );
    println!("  (Using TCP connect scan with 2s timeout)\n");

    println!(
        "  {:>6}  {:>10}  {:>12}  SERVICE",
        "PORT", "STATE", "CONNECT MS"
    );
    println!("  {:─>6}  {:─>10}  {:─>12}  {:─>15}", "", "", "", "");

    match scan_ports(target, &ports_to_scan) {
        Ok(results) => {
            let mut open_count = 0;
            let mut closed_count = 0;
            let mut filtered_count = 0;

            for result in &results {
                let status_str = match result.status {
                    PortStatus::Open => {
                        open_count += 1;
                        "open"
                    }
                    PortStatus::Closed => {
                        closed_count += 1;
                        "closed"
                    }
                    PortStatus::Filtered => {
                        filtered_count += 1;
                        "filtered"
                    }
                    PortStatus::Error => "error",
                };

                let connect_time = result
                    .connect_time_ms
                    .map(|t| format!("{:.2}", t))
                    .unwrap_or_else(|| "-".to_string());

                let service = result.service.as_deref().unwrap_or("-");

                // Only show open/filtered ports (like nmap default)
                if result.status == PortStatus::Open || result.status == PortStatus::Filtered {
                    println!(
                        "  {:>6}  {:>10}  {:>12}  {}",
                        result.port, status_str, connect_time, service
                    );
                }
            }

            println!();
            println!(
                "  Scan complete: {} open, {} closed, {} filtered",
                open_count, closed_count, filtered_count
            );
        }
        Err(e) => {
            println!("  ✗ Port scan failed: {}", e);
        }
    }

    // Quick single port check (netcat style)
    println!();
    println!("  Single port check (netcat style):");
    for port in [53, 80, 443] {
        match check_port(target, port, Duration::from_secs(2)) {
            Ok(open) => {
                let status = if open { "OPEN" } else { "CLOSED" };
                println!("    nc -zv {} {} → {}", target, port, status);
            }
            Err(e) => {
                println!("    nc -zv {} {} → ERROR: {}", target, port, e);
            }
        }
    }
    println!();
}

fn demo_service_names() {
    println!("┌─────────────────────────────────────────────────────────────────────┐");
    println!("│  6. WELL-KNOWN SERVICE PORTS                                        │");
    println!("└─────────────────────────────────────────────────────────────────────┘");

    println!("  Common ports recognized by ironmon:\n");
    println!("  {:>6}  SERVICE", "PORT");
    println!("  {:─>6}  {:─>20}", "", "");

    let well_known = [
        20, 21, 22, 23, 25, 53, 67, 68, 69, 80, 110, 123, 137, 138, 139, 143, 161, 162, 389, 443,
        445, 465, 514, 587, 636, 993, 995, 1433, 1521, 3306, 3389, 5432, 5900, 6379, 8080, 8443,
        27017,
    ];

    for port in well_known {
        if let Some(service) = get_service_name(port) {
            println!("  {:>6}  {}", port, service);
        }
    }

    println!();
    println!("  Common ports list has {} entries", common_ports().len());
    println!();
}

fn demo_latency(target: &str) {
    println!("┌─────────────────────────────────────────────────────────────────────┐");
    println!("│  7. LATENCY TEST - RTT Measurement                                  │");
    println!("└─────────────────────────────────────────────────────────────────────┘");

    println!("  Measuring latency to {} (10 samples)...\n", target);

    match latency_test(target, 10) {
        Ok(times) => {
            println!("  Sample RTTs:");
            for (i, time) in times.iter().enumerate() {
                let bar_len = (*time / 2.0).min(40.0) as usize;
                let bar: String = "█".repeat(bar_len);
                println!("    {:>2}. {:>8.2} ms  {}", i + 1, time, bar);
            }

            let sum: f64 = times.iter().sum();
            let avg = sum / times.len() as f64;
            let min = times.iter().cloned().fold(f64::INFINITY, f64::min);
            let max = times.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

            // Calculate jitter (stddev)
            let variance: f64 =
                times.iter().map(|t| (t - avg).powi(2)).sum::<f64>() / times.len() as f64;
            let jitter = variance.sqrt();

            println!();
            println!("  Statistics:");
            println!("    Min: {:.2} ms", min);
            println!("    Max: {:.2} ms", max);
            println!("    Avg: {:.2} ms", avg);
            println!("    Jitter: {:.2} ms", jitter);
        }
        Err(e) => {
            println!("  ✗ Latency test failed: {}", e);
        }
    }
    println!();
}
