// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024 NervoSys

//! `net_context`'s parsers, run against the real tools on each CI runner.
//!
//! The unit tests feed the parsers captured output. These feed them whatever
//! the runner's own `ip`, `ifconfig`, `netstat` or PowerShell prints today, and
//! check the result against a second, independent answer from the same
//! machine: an interface list, a route the tool itself printed. A parser that
//! drifts out of step with a tool's format fails here rather than quietly
//! reporting a machine with no network.

use ironmonitor::net_context;

fn run(program: &str, args: &[&str]) -> String {
    let output = std::process::Command::new(program)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("{program} could not be run: {e}"));
    assert!(output.status.success(), "{program} {args:?} failed");
    String::from_utf8(output.stdout).expect("UTF-8 output")
}

/// Every interface the kernel lists is parsed, with its loopback address.
#[cfg(target_os = "linux")]
#[test]
fn ip_addr_is_parsed_interface_for_interface() {
    let json: serde_json::Value =
        serde_json::from_str(&run("ip", &["-j", "addr", "show"])).unwrap();
    let adapters = net_context::parse_ip_addr(&json);
    let names: Vec<&str> = json
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|i| i["ifname"].as_str())
        .collect();
    assert_eq!(
        adapters.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(),
        names
    );
    let lo = adapters
        .iter()
        .find(|a| a.name == "lo")
        .expect("a loopback interface");
    assert!(
        lo.addresses.iter().any(|a| a.address == "127.0.0.1"),
        "{lo:?}"
    );
}

/// A runner reaches the internet, so it has a default route, and the parser
/// finds the gateway `ip route` printed.
#[cfg(target_os = "linux")]
#[test]
fn the_default_gateway_ip_prints_is_found() {
    let routes: serde_json::Value =
        serde_json::from_str(&run("ip", &["-j", "route", "show", "default"])).unwrap();
    let expected: Vec<&str> = routes
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|r| r["gateway"].as_str())
        .collect();
    let gateways = net_context::parse_ip_routes(&routes, &serde_json::Value::Null);
    assert_eq!(
        gateways
            .iter()
            .map(|g| g.address.as_str())
            .collect::<Vec<_>>(),
        expected
    );
    let ctx = net_context::read();
    assert!(ctx.adapters.is_ok(), "{:?}", ctx.adapters);
    assert!(ctx.gateways.is_ok(), "{:?}", ctx.gateways);
}

/// `ifconfig -l` is the interface list in one line; the block parser must
/// find the same interfaces.
#[cfg(target_os = "macos")]
#[test]
fn ifconfig_is_parsed_interface_for_interface() {
    let adapters = net_context::parse_ifconfig(&run("ifconfig", &[]));
    let mut parsed: Vec<String> = adapters.iter().map(|a| a.name.clone()).collect();
    let mut listed: Vec<String> = run("ifconfig", &["-l"])
        .split_whitespace()
        .map(str::to_string)
        .collect();
    parsed.sort();
    listed.sort();
    assert_eq!(parsed, listed);
    let lo = adapters.iter().find(|a| a.name == "lo0").expect("lo0");
    assert!(
        lo.addresses.iter().any(|a| a.address == "127.0.0.1"),
        "{lo:?}"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn the_default_gateway_netstat_prints_is_found() {
    let netstat = run("netstat", &["-rn", "-f", "inet"]);
    let expected = netstat
        .lines()
        .find(|l| l.starts_with("default"))
        .and_then(|l| l.split_whitespace().nth(1))
        .expect("an IPv4 default route")
        .to_string();
    let gateways = net_context::parse_netstat_defaults(&netstat, "");
    assert!(
        gateways.iter().any(|g| g.address == expected),
        "{gateways:?} lacks {expected}"
    );
    let ctx = net_context::read();
    assert!(ctx.dns_servers.is_ok(), "{:?}", ctx.dns_servers);
    assert!(ctx.wifi.is_ok(), "{:?}", ctx.wifi);
}

/// The PowerShell query returns the adapters `Get-NetAdapter` lists, by name.
#[cfg(windows)]
#[test]
fn the_powershell_query_finds_every_adapter() {
    let names = run(
        "powershell",
        &[
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-NetAdapter | ForEach-Object { $_.Name }",
        ],
    );
    let mut listed: Vec<&str> = names
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let ctx = net_context::read();
    let adapters = ctx.adapters.expect("the network query runs");
    let mut parsed: Vec<&str> = adapters.iter().map(|a| a.name.as_str()).collect();
    listed.sort_unstable();
    parsed.sort_unstable();
    assert_eq!(parsed, listed);
    assert!(ctx.gateways.is_ok() && ctx.dns_servers.is_ok());
}
