// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (c) 2024 NervoSys

//! Where this machine sits on the network: each adapter's addresses and driver,
//! the default gateways and their link-layer addresses, the DNS servers, and the
//! Wi-Fi connection -- SSID, BSSID, radio, security, signal and rates.
//!
//! `network_monitor` counts traffic per interface. It does not read addresses on
//! Windows ("Would need GetIpAddrTable") or MACs on macOS, and nothing in the
//! crate read routes, DNS or Wi-Fi at all. This module reads them from the tools
//! each OS ships:
//!
//! | | Windows | Linux | macOS |
//! |---|---|---|---|
//! | adapters, addresses | `Get-NetAdapter`, `Get-NetIPAddress` | `ip -j addr` | `ifconfig` |
//! | gateways | `Get-NetRoute` | `ip -j route` | `netstat -rn` |
//! | gateway MAC | `Get-NetNeighbor` | `ip -j neigh` | `arp -an` |
//! | DNS | `Get-DnsClientServerAddress` | `/etc/resolv.conf` | `scutil --dns` |
//! | Wi-Fi | `netsh wlan show interfaces` | `iw dev`, `iw dev <if> link` | `system_profiler SPAirPortDataType` |
//!
//! Every parser is a pure function over the tool's text, tested against captured
//! output. Each part carries its own failure, so a missing `iw` does not hide the
//! routes.

use serde::{Deserialize, Serialize};

/// One address on an adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpAddress {
    /// The address, without a prefix length or a zone suffix.
    pub address: String,
    /// Prefix length, where reported.
    pub prefix_length: Option<u8>,
    /// `ipv4` or `ipv6`.
    pub family: String,
}

/// One network adapter, physical or virtual.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkAdapter {
    /// The OS's name for it: `Ethernet`, `eth0`, `en0`.
    pub name: String,
    /// The driver's description of the hardware, where the OS gives one.
    pub description: Option<String>,
    /// Operational state as the OS words it: `Up`, `Disconnected`, `up`, `active`.
    pub status: Option<String>,
    /// Link-layer address, colon-separated lowercase hex.
    pub mac: Option<String>,
    /// MTU in bytes.
    pub mtu: Option<u32>,
    /// Negotiated link speed in bits per second, where the OS reports one. This
    /// is the link's capacity, not traffic on it. It used to be written into
    /// `receive_bps` and `transmit_bps`, which read as throughput; no rate is
    /// read here, because a rate needs two samples of a counter.
    pub link_speed_bps: Option<u64>,
    /// Media type, e.g. `802.3` or `Native 802.11`.
    pub media_type: Option<String>,
    /// Whether the OS calls it virtual (a VPN, a hypervisor switch).
    pub is_virtual: Option<bool>,
    /// The kernel driver's name (Linux) or provider (Windows).
    pub driver: Option<String>,
    pub driver_version: Option<String>,
    pub driver_date: Option<String>,
    /// Addresses assigned to it.
    pub addresses: Vec<IpAddress>,
    /// DNS servers configured on it (Windows only; elsewhere DNS is global).
    pub dns_servers: Vec<String>,
    /// Connection-specific DNS suffix (Windows).
    pub dns_suffix: Option<String>,
}

/// One default route.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gateway {
    /// The next hop's address.
    pub address: String,
    /// `ipv4` or `ipv6`.
    pub family: String,
    /// The interface the route leaves by.
    pub interface: Option<String>,
    /// Route metric, where reported.
    pub metric: Option<u32>,
    /// The next hop's link-layer address, from the neighbour table. `None`
    /// where the table has no entry for it: a VPN's gateway, or one not yet
    /// resolved.
    pub mac: Option<String>,
}

/// One Wi-Fi interface and, when connected, its connection.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WifiInterface {
    pub interface: String,
    pub description: Option<String>,
    pub mac: Option<String>,
    /// The interface's state as the tool words it: `connected`,
    /// `disconnected`, `Not connected.`
    pub state: Option<String>,
    /// Whether it is associated with an access point.
    pub connected: Option<bool>,
    pub ssid: Option<String>,
    pub bssid: Option<String>,
    /// e.g. `802.11ax`.
    pub radio_type: Option<String>,
    pub authentication: Option<String>,
    pub cipher: Option<String>,
    /// e.g. `5 GHz`.
    pub band: Option<String>,
    pub channel: Option<u32>,
    /// Centre frequency in MHz.
    pub frequency_mhz: Option<u32>,
    /// Channel width in MHz.
    pub channel_width_mhz: Option<u32>,
    /// Signal quality as a percentage (Windows reports this, not dBm).
    pub signal_percent: Option<u8>,
    /// Received signal strength in dBm.
    pub rssi_dbm: Option<i32>,
    /// Noise floor in dBm.
    pub noise_dbm: Option<i32>,
    /// Link rates in Mbit/s.
    pub receive_rate_mbps: Option<f64>,
    pub transmit_rate_mbps: Option<f64>,
    /// The saved profile the connection uses (Windows).
    pub profile: Option<String>,
    pub country_code: Option<String>,
}

/// Everything this module reads. Each part fails on its own.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NetworkContext {
    pub adapters: Result<Vec<NetworkAdapter>, String>,
    pub gateways: Result<Vec<Gateway>, String>,
    /// DNS servers in resolution order, across all interfaces.
    pub dns_servers: Result<Vec<String>, String>,
    /// DNS search domains.
    pub search_domains: Result<Vec<String>, String>,
    pub wifi: Result<Vec<WifiInterface>, String>,
}

/// Normalise a MAC to colon-separated lowercase hex. `None` for anything that
/// is not six octets, or is all zeros (an adapter with no address).
pub fn normalize_mac(raw: &str) -> Option<String> {
    let hex: String = raw
        .trim()
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .collect::<String>()
        .to_ascii_lowercase();
    let separators = raw
        .trim()
        .chars()
        .filter(|c| matches!(c, ':' | '-' | '.'))
        .count();
    if hex.len() != 12 || (separators != 0 && separators != 5 && separators != 2) {
        return None;
    }
    if hex.chars().all(|c| c == '0') {
        return None;
    }
    Some(
        hex.as_bytes()
            .chunks(2)
            .map(|pair| std::str::from_utf8(pair).unwrap_or("00"))
            .collect::<Vec<_>>()
            .join(":"),
    )
}

fn family_of(address: &str) -> &'static str {
    if address.contains(':') {
        "ipv6"
    } else {
        "ipv4"
    }
}

/// Strip an IPv6 zone (`fe80::1%en0`) and a prefix (`/64`).
fn bare_address(raw: &str) -> String {
    let no_prefix = raw.split('/').next().unwrap_or(raw);
    no_prefix
        .split('%')
        .next()
        .unwrap_or(no_prefix)
        .trim()
        .to_string()
}

fn push_unique(list: &mut Vec<String>, value: String) {
    if !value.is_empty() && !list.contains(&value) {
        list.push(value);
    }
}

fn nonempty(value: &str) -> Option<String> {
    let v = value.trim();
    (!v.is_empty()).then(|| v.to_string())
}

// ─── Windows ────────────────────────────────────────────────────────────────

/// The one PowerShell query behind the Windows adapters, routes, neighbours
/// and DNS. Enums are stringified, since `ConvertTo-Json` writes them as
/// numbers.
pub const WINDOWS_QUERY: &str = concat!(
    "$ErrorActionPreference='SilentlyContinue';",
    "$a=@(Get-NetAdapter | Select-Object Name,InterfaceDescription,ifIndex,",
    "@{n='Status';e={\"$($_.Status)\"}},MacAddress,MtuSize,ReceiveLinkSpeed,TransmitLinkSpeed,",
    "MediaType,Virtual,DriverProvider,DriverVersion,",
    "@{n='DriverDate';e={\"$($_.DriverDate)\"}});",
    "$i=@(Get-NetIPAddress | Select-Object InterfaceIndex,IPAddress,PrefixLength,",
    "@{n='AddressFamily';e={\"$($_.AddressFamily)\"}});",
    "$r=@(Get-NetRoute -DestinationPrefix '0.0.0.0/0','::/0' | Select-Object InterfaceIndex,",
    "InterfaceAlias,NextHop,RouteMetric,@{n='AddressFamily';e={\"$($_.AddressFamily)\"}});",
    "$n=@(Get-NetNeighbor | Select-Object InterfaceIndex,IPAddress,LinkLayerAddress,",
    "@{n='State';e={\"$($_.State)\"}});",
    "$d=@(Get-DnsClientServerAddress | Select-Object InterfaceIndex,ServerAddresses);",
    "$s=@(Get-DnsClient | Select-Object InterfaceIndex,ConnectionSpecificSuffix);",
    "$g=@((Get-DnsClientGlobalSetting).SuffixSearchList);",
    "[pscustomobject]@{adapters=$a;addresses=$i;routes=$r;neighbors=$n;dns=$d;",
    "suffixes=$s;search=$g} | ConvertTo-Json -Depth 4 -Compress"
);

fn json_array<'a>(value: &'a serde_json::Value, key: &str) -> Vec<&'a serde_json::Value> {
    match value.get(key) {
        Some(serde_json::Value::Array(items)) => items.iter().collect(),
        Some(item @ serde_json::Value::Object(_)) => vec![item],
        _ => Vec::new(),
    }
}

fn json_str(value: &serde_json::Value, key: &str) -> Option<String> {
    value.get(key).and_then(|v| v.as_str()).and_then(nonempty)
}

fn json_u64(value: &serde_json::Value, key: &str) -> Option<u64> {
    value.get(key).and_then(|v| v.as_u64())
}

fn json_strings(value: &serde_json::Value, key: &str) -> Vec<String> {
    match value.get(key) {
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .filter_map(|v| v.as_str().and_then(nonempty))
            .collect(),
        Some(serde_json::Value::String(s)) => nonempty(s).into_iter().collect(),
        _ => Vec::new(),
    }
}

/// Parse [`WINDOWS_QUERY`]'s output into adapters, gateways, DNS servers and
/// search domains.
#[allow(clippy::type_complexity)]
pub fn parse_windows(
    json: &serde_json::Value,
) -> (Vec<NetworkAdapter>, Vec<Gateway>, Vec<String>, Vec<String>) {
    use std::collections::HashMap;
    let mut by_index: HashMap<u64, usize> = HashMap::new();
    let mut adapters: Vec<NetworkAdapter> = Vec::new();
    for item in json_array(json, "adapters") {
        let Some(name) = json_str(item, "Name") else {
            continue;
        };
        let status = json_str(item, "Status");
        let up = status.as_deref() == Some("Up");
        // A disconnected adapter reports a link rate of 0, which is no link,
        // not a rate.
        let rate = |key: &str| json_u64(item, key).filter(|bps| up && *bps > 0);
        if let Some(index) = json_u64(item, "ifIndex") {
            by_index.insert(index, adapters.len());
        }
        adapters.push(NetworkAdapter {
            name,
            description: json_str(item, "InterfaceDescription"),
            status,
            mac: json_str(item, "MacAddress").and_then(|m| normalize_mac(&m)),
            mtu: json_u64(item, "MtuSize")
                .filter(|m| *m > 0)
                .and_then(|m| u32::try_from(m).ok()),
            // Both directions must report the same speed, or neither is taken:
            // one figure would otherwise stand for a link whose two ends differ.
            link_speed_bps: rate("ReceiveLinkSpeed")
                .filter(|rx| rate("TransmitLinkSpeed") == Some(*rx)),
            media_type: json_str(item, "MediaType"),
            is_virtual: item.get("Virtual").and_then(|v| v.as_bool()),
            driver: json_str(item, "DriverProvider"),
            driver_version: json_str(item, "DriverVersion"),
            driver_date: json_str(item, "DriverDate"),
            ..Default::default()
        });
    }
    let adapter_at = |index: Option<u64>| index.and_then(|i| by_index.get(&i).copied());

    for item in json_array(json, "addresses") {
        let (Some(slot), Some(address)) = (
            adapter_at(json_u64(item, "InterfaceIndex")),
            json_str(item, "IPAddress"),
        ) else {
            continue;
        };
        let address = bare_address(&address);
        adapters[slot].addresses.push(IpAddress {
            family: family_of(&address).to_string(),
            prefix_length: json_u64(item, "PrefixLength").and_then(|p| u8::try_from(p).ok()),
            address,
        });
    }

    let mut dns_servers = Vec::new();
    for item in json_array(json, "dns") {
        let servers = json_strings(item, "ServerAddresses");
        let slot = adapter_at(json_u64(item, "InterfaceIndex"));
        if let Some(slot) = slot {
            for server in &servers {
                push_unique(&mut adapters[slot].dns_servers, server.clone());
            }
        }
        // The machine's resolvers are the ones on links that are up. A
        // disconnected adapter keeps its last DHCP servers, and an
        // unconfigured one lists Windows' fec0:0:0:ffff::1-3 placeholders;
        // neither answers a query. Interfaces Get-NetAdapter does not list,
        // such as a VPN tunnel, are kept.
        let up = slot.is_none_or(|s| adapters[s].status.as_deref() == Some("Up"));
        if up {
            for server in servers {
                push_unique(&mut dns_servers, server);
            }
        }
    }
    for item in json_array(json, "suffixes") {
        if let Some(slot) = adapter_at(json_u64(item, "InterfaceIndex")) {
            adapters[slot].dns_suffix = json_str(item, "ConnectionSpecificSuffix");
        }
    }

    // The neighbour table, keyed by interface and address, for gateway MACs.
    let mut neighbours: HashMap<(u64, String), String> = HashMap::new();
    for item in json_array(json, "neighbors") {
        let state = json_str(item, "State").unwrap_or_default();
        if matches!(state.as_str(), "Unreachable" | "Incomplete") {
            continue;
        }
        if let (Some(index), Some(ip), Some(mac)) = (
            json_u64(item, "InterfaceIndex"),
            json_str(item, "IPAddress"),
            json_str(item, "LinkLayerAddress").and_then(|m| normalize_mac(&m)),
        ) {
            neighbours.insert((index, bare_address(&ip)), mac);
        }
    }

    let mut gateways = Vec::new();
    for item in json_array(json, "routes") {
        let Some(address) = json_str(item, "NextHop").map(|a| bare_address(&a)) else {
            continue;
        };
        // On-link routes have an unspecified next hop; there is no gateway.
        if address == "0.0.0.0" || address == "::" {
            continue;
        }
        let index = json_u64(item, "InterfaceIndex");
        gateways.push(Gateway {
            family: family_of(&address).to_string(),
            interface: json_str(item, "InterfaceAlias")
                .or_else(|| adapter_at(index).map(|slot| adapters[slot].name.clone())),
            metric: json_u64(item, "RouteMetric").and_then(|m| u32::try_from(m).ok()),
            mac: index.and_then(|i| neighbours.get(&(i, address.clone())).cloned()),
            address,
        });
    }

    let search = json_strings(json, "search");
    (adapters, gateways, dns_servers, search)
}

/// `netsh wlan show interfaces` prints this, and nothing else, where Windows
/// 11 24H2's location consent has not been given.
const NETSH_LOCATION_REFUSAL: &str = "location permission";

/// Parse `netsh wlan show interfaces`. English output only: `netsh` localises
/// its labels, and a label it does not recognise is left unread.
pub fn parse_netsh_wlan(text: &str) -> Result<Vec<WifiInterface>, String> {
    if text.to_ascii_lowercase().contains(NETSH_LOCATION_REFUSAL) {
        return Err(concat!(
            "Windows withholds Wi-Fi details until location access is allowed for ",
            "desktop apps (Settings > Privacy & security > Location)"
        )
        .into());
    }
    let mut interfaces: Vec<WifiInterface> = Vec::new();
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        if key == "Name" {
            interfaces.push(WifiInterface {
                interface: value.to_string(),
                ..Default::default()
            });
            continue;
        }
        let Some(wifi) = interfaces.last_mut() else {
            continue;
        };
        let number = |v: &str| {
            v.trim_end_matches('%')
                .split_whitespace()
                .next()
                .and_then(|n| n.parse::<f64>().ok())
        };
        match key {
            "Description" => wifi.description = nonempty(value),
            "Physical address" => wifi.mac = normalize_mac(value),
            "State" => {
                wifi.state = nonempty(value);
                wifi.connected = Some(value.eq_ignore_ascii_case("connected"));
            }
            "SSID" => wifi.ssid = nonempty(value),
            "BSSID" | "AP BSSID" => wifi.bssid = normalize_mac(value),
            "Radio type" => wifi.radio_type = nonempty(value),
            "Authentication" => wifi.authentication = nonempty(value),
            "Cipher" => wifi.cipher = nonempty(value),
            "Band" => wifi.band = nonempty(value),
            "Channel" => wifi.channel = value.parse().ok(),
            "Receive rate (Mbps)" => wifi.receive_rate_mbps = number(value),
            "Transmit rate (Mbps)" => wifi.transmit_rate_mbps = number(value),
            "Signal" => {
                wifi.signal_percent = number(value)
                    .filter(|p| (0.0..=100.0).contains(p))
                    .map(|p| p as u8)
            }
            "Rssi" => wifi.rssi_dbm = value.parse().ok(),
            "Profile" => wifi.profile = nonempty(value),
            _ => {}
        }
    }
    // Output with no `Name` line is not "no Wi-Fi adapter". The labels are
    // matched in English only, so a localised listing that names no interface
    // in a key this reader knows used to come back as an empty list, and the
    // ontology reported that the machine has no Wi-Fi.
    if interfaces.is_empty() && !text.trim().is_empty() {
        return Err(concat!(
            "netsh wlan printed output with no interface this reader recognises ",
            "(its labels are English only)"
        )
        .into());
    }
    Ok(interfaces)
}

// ─── Linux ──────────────────────────────────────────────────────────────────

/// Parse `ip -j addr show`.
pub fn parse_ip_addr(json: &serde_json::Value) -> Vec<NetworkAdapter> {
    let Some(items) = json.as_array() else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let name = json_str(item, "ifname")?;
            let addresses = item
                .get("addr_info")
                .and_then(|a| a.as_array())
                .map(|infos| {
                    infos
                        .iter()
                        .filter_map(|info| {
                            let address = bare_address(&json_str(info, "local")?);
                            Some(IpAddress {
                                family: match json_str(info, "family").as_deref() {
                                    Some("inet6") => "ipv6".to_string(),
                                    Some("inet") => "ipv4".to_string(),
                                    _ => family_of(&address).to_string(),
                                },
                                prefix_length: json_u64(info, "prefixlen")
                                    .and_then(|p| u8::try_from(p).ok()),
                                address,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            Some(NetworkAdapter {
                status: json_str(item, "operstate"),
                mac: json_str(item, "address").and_then(|m| normalize_mac(&m)),
                mtu: json_u64(item, "mtu").and_then(|m| u32::try_from(m).ok()),
                media_type: json_str(item, "link_type"),
                addresses,
                name,
                ..Default::default()
            })
        })
        .collect()
}

/// Parse `ip -j route show default` (or `ip -j -6 route show default`), with
/// `ip -j neigh` for the gateways' MACs.
pub fn parse_ip_routes(routes: &serde_json::Value, neighbours: &serde_json::Value) -> Vec<Gateway> {
    let neigh: Vec<(String, String, String)> = neighbours
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|n| {
                    Some((
                        json_str(n, "dst")?,
                        json_str(n, "dev").unwrap_or_default(),
                        json_str(n, "lladdr").and_then(|m| normalize_mac(&m))?,
                    ))
                })
                .collect()
        })
        .unwrap_or_default();
    routes
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|route| {
                    let address = bare_address(&json_str(route, "gateway")?);
                    let interface = json_str(route, "dev");
                    let mac = neigh
                        .iter()
                        .find(|(dst, dev, _)| {
                            *dst == address && interface.as_deref().is_none_or(|i| i == dev)
                        })
                        .map(|(_, _, mac)| mac.clone());
                    Some(Gateway {
                        family: family_of(&address).to_string(),
                        metric: json_u64(route, "metric").and_then(|m| u32::try_from(m).ok()),
                        interface,
                        mac,
                        address,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Parse `/etc/resolv.conf` into nameservers and search domains.
pub fn parse_resolv_conf(text: &str) -> (Vec<String>, Vec<String>) {
    let mut servers = Vec::new();
    let mut search = Vec::new();
    for line in text.lines() {
        let line = line.split(['#', ';']).next().unwrap_or("").trim();
        let mut words = line.split_whitespace();
        match words.next() {
            Some("nameserver") => {
                if let Some(server) = words.next() {
                    push_unique(&mut servers, bare_address(server));
                }
            }
            Some("search") | Some("domain") => {
                for domain in words {
                    push_unique(&mut search, domain.to_string());
                }
            }
            _ => {}
        }
    }
    (servers, search)
}

/// The wireless interfaces `iw dev` lists.
pub fn parse_iw_dev(text: &str) -> Vec<(String, Option<String>)> {
    let mut interfaces: Vec<(String, Option<String>)> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix("Interface ") {
            interfaces.push((name.trim().to_string(), None));
        } else if let Some(addr) = line.strip_prefix("addr ") {
            if let Some(last) = interfaces.last_mut() {
                last.1 = normalize_mac(addr);
            }
        }
    }
    interfaces
}

/// Parse `iw dev <if> link` into the connection fields of `wifi`.
pub fn parse_iw_link(text: &str, wifi: &mut WifiInterface) {
    let first = text.lines().next().unwrap_or("").trim();
    if first.starts_with("Not connected") {
        wifi.state = Some("not connected".into());
        wifi.connected = Some(false);
        return;
    }
    if let Some(rest) = first.strip_prefix("Connected to ") {
        wifi.state = Some("connected".into());
        wifi.connected = Some(true);
        wifi.bssid = rest.split_whitespace().next().and_then(normalize_mac);
    }
    let mbit = |v: &str| {
        v.split_whitespace()
            .next()
            .and_then(|n| n.parse::<f64>().ok())
    };
    for line in text.lines().skip(1) {
        let Some((key, value)) = line.trim().split_once(':') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "SSID" => wifi.ssid = nonempty(value),
            "freq" => {
                wifi.frequency_mhz = value
                    .split_whitespace()
                    .next()
                    .and_then(|f| f.parse::<f64>().ok())
                    .map(|f| f as u32)
            }
            "signal" => {
                wifi.rssi_dbm = value
                    .split_whitespace()
                    .next()
                    .and_then(|s| s.parse::<i32>().ok())
            }
            "rx bitrate" => wifi.receive_rate_mbps = mbit(value),
            "tx bitrate" => wifi.transmit_rate_mbps = mbit(value),
            _ => {}
        }
    }
    if let Some(freq) = wifi.frequency_mhz {
        wifi.band = band_of(freq).map(str::to_string);
        wifi.channel = channel_of(freq);
    }
}

/// The band a centre frequency lies in.
pub fn band_of(freq_mhz: u32) -> Option<&'static str> {
    match freq_mhz {
        2400..=2500 => Some("2.4 GHz"),
        5150..=5925 => Some("5 GHz"),
        5926..=7125 => Some("6 GHz"),
        _ => None,
    }
}

/// The IEEE channel number of a centre frequency (802.11-2020 Annex E).
pub fn channel_of(freq_mhz: u32) -> Option<u32> {
    match freq_mhz {
        2484 => Some(14),
        2412..=2472 => Some((freq_mhz - 2407) / 5),
        5160..=5885 => Some((freq_mhz - 5000) / 5),
        5955..=7115 => Some((freq_mhz - 5950) / 5),
        _ => None,
    }
}

// ─── macOS ──────────────────────────────────────────────────────────────────

/// Parse `ifconfig`.
pub fn parse_ifconfig(text: &str) -> Vec<NetworkAdapter> {
    let mut adapters: Vec<NetworkAdapter> = Vec::new();
    for line in text.lines() {
        if !line.starts_with(['\t', ' ']) {
            let Some((name, rest)) = line.split_once(": ") else {
                continue;
            };
            let mtu = rest
                .split_whitespace()
                .skip_while(|w| *w != "mtu")
                .nth(1)
                .and_then(|m| m.parse().ok());
            adapters.push(NetworkAdapter {
                name: name.to_string(),
                mtu,
                ..Default::default()
            });
            continue;
        }
        let Some(adapter) = adapters.last_mut() else {
            continue;
        };
        let words: Vec<&str> = line.split_whitespace().collect();
        match words.as_slice() {
            ["ether", mac, ..] => adapter.mac = normalize_mac(mac),
            ["inet", address, rest @ ..] => {
                let prefix = rest
                    .iter()
                    .position(|w| *w == "netmask")
                    .and_then(|i| rest.get(i + 1))
                    .and_then(|mask| u32::from_str_radix(mask.trim_start_matches("0x"), 16).ok())
                    .map(|mask| mask.count_ones() as u8);
                adapter.addresses.push(IpAddress {
                    address: bare_address(address),
                    prefix_length: prefix,
                    family: "ipv4".into(),
                });
            }
            ["inet6", address, rest @ ..] => {
                let prefix = rest
                    .iter()
                    .position(|w| *w == "prefixlen")
                    .and_then(|i| rest.get(i + 1))
                    .and_then(|p| p.parse().ok());
                adapter.addresses.push(IpAddress {
                    address: bare_address(address),
                    prefix_length: prefix,
                    family: "ipv6".into(),
                });
            }
            ["status:", status, ..] => adapter.status = Some(status.to_string()),
            ["media:", ..] => {
                adapter.media_type = nonempty(line.trim().trim_start_matches("media:"))
            }
            _ => {}
        }
    }
    adapters
}

/// Parse the `default` rows of `netstat -rn`, with `arp -an` for MACs.
pub fn parse_netstat_defaults(netstat: &str, arp: &str) -> Vec<Gateway> {
    // `? (192.168.1.1) at aa:bb:cc:dd:ee:ff on en0 ifscope [ethernet]`
    // A row with no `on <device>` is skipped. It used to get an empty device,
    // which the match below read as "any interface", so its MAC could be
    // attached to a gateway on an interface it was never seen on.
    let neighbours: Vec<(String, String, String)> = arp
        .lines()
        .filter_map(|line| {
            let ip = line.split('(').nth(1)?.split(')').next()?.to_string();
            let mut words = line.split_whitespace();
            let mac = words.by_ref().skip_while(|w| *w != "at").nth(1)?;
            let dev = words.skip_while(|w| *w != "on").nth(1)?;
            Some((ip, dev.to_string(), normalize_mac(mac)?))
        })
        .collect();
    netstat
        .lines()
        .filter_map(|line| {
            let words: Vec<&str> = line.split_whitespace().collect();
            if words.first() != Some(&"default") || words.len() < 4 {
                return None;
            }
            let address = bare_address(words[1]);
            // `link#4` is an on-link route, not a next hop.
            if address.starts_with("link#") || address.contains(':') && address.len() == 17 {
                return None;
            }
            let interface = words[3].to_string();
            let mac = neighbours
                .iter()
                .find(|(ip, dev, _)| *ip == address && *dev == interface)
                .map(|(_, _, mac)| mac.clone());
            Some(Gateway {
                family: family_of(&address).to_string(),
                interface: Some(interface),
                metric: None,
                mac,
                address,
            })
        })
        .collect()
}

/// Parse `scutil --dns` into nameservers and search domains, in order.
pub fn parse_scutil_dns(text: &str) -> (Vec<String>, Vec<String>) {
    let mut servers = Vec::new();
    let mut search = Vec::new();
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        if key.starts_with("nameserver[") {
            push_unique(&mut servers, bare_address(value.trim()));
        } else if key.starts_with("search domain[") {
            push_unique(&mut search, value.trim().to_string());
        }
    }
    (servers, search)
}

/// Parse `system_profiler SPAirPortDataType -json`.
///
/// macOS 14 and later print `<redacted>` for the SSID and BSSID unless the
/// asking app has location access; that is read as unread, not as a name.
pub fn parse_airport(json: &serde_json::Value) -> Vec<WifiInterface> {
    let redacted = |v: Option<String>| v.filter(|s| !s.contains("redacted"));
    let mut out = Vec::new();
    for item in json["SPAirPortDataType"].as_array().into_iter().flatten() {
        for iface in item["spairport_airport_interfaces"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let Some(name) = json_str(iface, "_name") else {
                continue;
            };
            let status = json_str(iface, "spairport_status_information");
            let current = &iface["spairport_current_network_information"];
            let connected = current.is_object();
            let mut wifi = WifiInterface {
                interface: name,
                description: json_str(iface, "spairport_wireless_card_type"),
                mac: json_str(iface, "spairport_wireless_mac_address")
                    .and_then(|m| normalize_mac(&m)),
                state: status
                    .as_deref()
                    .map(|s| s.trim_start_matches("spairport_status_").to_string()),
                connected: Some(connected),
                ..Default::default()
            };
            if connected {
                wifi.ssid = redacted(json_str(current, "_name"));
                wifi.bssid = redacted(json_str(current, "spairport_network_bssid"))
                    .and_then(|b| normalize_mac(&b));
                wifi.radio_type = json_str(current, "spairport_network_phymode");
                wifi.authentication = json_str(current, "spairport_security_mode").map(|s| {
                    s.trim_start_matches("spairport_security_mode_")
                        .replace('_', " ")
                });
                wifi.country_code = json_str(current, "spairport_network_country_code");
                wifi.transmit_rate_mbps = current
                    .get("spairport_network_rate")
                    .and_then(|r| r.as_f64());
                // "36 (5GHz, 80MHz)"
                if let Some(channel) = json_str(current, "spairport_network_channel") {
                    wifi.channel = channel
                        .split_whitespace()
                        .next()
                        .and_then(|c| c.parse().ok());
                    let detail = channel
                        .split('(')
                        .nth(1)
                        .unwrap_or("")
                        .trim_end_matches(')');
                    for part in detail.split(',').map(str::trim) {
                        if let Some(ghz) = part.strip_suffix("GHz") {
                            wifi.band = Some(format!("{ghz} GHz"));
                        } else if let Some(mhz) = part.strip_suffix("MHz") {
                            wifi.channel_width_mhz = mhz.parse().ok();
                        }
                    }
                }
                // "-51 dBm / -93 dBm"
                if let Some(sn) = json_str(current, "spairport_signal_noise") {
                    let mut figures = sn
                        .split('/')
                        .map(|p| p.split_whitespace().next().and_then(|n| n.parse().ok()));
                    wifi.rssi_dbm = figures.next().flatten();
                    wifi.noise_dbm = figures.next().flatten();
                }
            }
            out.push(wifi);
        }
    }
    out
}

// ─── Readers ────────────────────────────────────────────────────────────────

fn capture(program: &str, args: &[&str]) -> Result<String, String> {
    crate::core::command::capture(program, args).map_err(|e| e.to_string())
}

/// Read everything [`NetworkContext`] describes.
pub fn read() -> NetworkContext {
    #[cfg(windows)]
    {
        let base = crate::core::command::capture_json(
            "powershell",
            &["-NoProfile", "-NonInteractive", "-Command", WINDOWS_QUERY],
        )
        .map_err(|e| e.to_string())
        .and_then(|v| v.ok_or_else(|| "the network query printed nothing".to_string()));
        let wifi = match capture("netsh", &["wlan", "show", "interfaces"]) {
            Ok(text) => parse_netsh_wlan(&text),
            // `netsh` exits non-zero when the WLAN service is not running,
            // which is how a machine with no Wi-Fi adapter answers.
            Err(e) => Err(format!("netsh wlan could not be read: {e}")),
        };
        match base {
            Ok(json) => {
                let (adapters, gateways, dns, search) = parse_windows(&json);
                NetworkContext {
                    adapters: Ok(adapters),
                    gateways: Ok(gateways),
                    dns_servers: Ok(dns),
                    search_domains: Ok(search),
                    wifi,
                }
            }
            Err(e) => NetworkContext {
                adapters: Err(e.clone()),
                gateways: Err(e.clone()),
                dns_servers: Err(e.clone()),
                search_domains: Err(e),
                wifi,
            },
        }
    }

    #[cfg(target_os = "linux")]
    {
        let ip_json = |args: &[&str]| -> Result<serde_json::Value, String> {
            let text = capture("ip", args)?;
            serde_json::from_str(&text)
                .map_err(|e| format!("ip {} is not JSON: {e}", args.join(" ")))
        };
        let mut adapters = ip_json(&["-j", "addr", "show"]).map(|j| parse_ip_addr(&j));
        if let Ok(list) = adapters.as_mut() {
            for adapter in list.iter_mut() {
                let link = format!("/sys/class/net/{}/device/driver", adapter.name);
                adapter.driver = std::fs::read_link(&link)
                    .ok()
                    .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()));
                let speed =
                    std::fs::read_to_string(format!("/sys/class/net/{}/speed", adapter.name))
                        .ok()
                        .and_then(|s| s.trim().parse::<i64>().ok())
                        .filter(|mbps| *mbps > 0)
                        .map(|mbps| mbps as u64 * 1_000_000);
                adapter.link_speed_bps = speed;
            }
        }
        let neigh = ip_json(&["-j", "neigh"]).unwrap_or(serde_json::Value::Null);
        let gateways = ip_json(&["-j", "route", "show", "default"]).map(|v4| {
            let mut all = parse_ip_routes(&v4, &neigh);
            if let Ok(v6) = ip_json(&["-j", "-6", "route", "show", "default"]) {
                all.extend(parse_ip_routes(&v6, &neigh));
            }
            all
        });
        let resolv = std::fs::read_to_string("/etc/resolv.conf")
            .map_err(|e| format!("/etc/resolv.conf could not be read: {e}"))
            .map(|t| parse_resolv_conf(&t));
        let wifi = capture("iw", &["dev"]).map(|text| {
            parse_iw_dev(&text)
                .into_iter()
                .map(|(name, mac)| {
                    let mut wifi = WifiInterface {
                        interface: name.clone(),
                        mac,
                        ..Default::default()
                    };
                    match capture("iw", &["dev", &name, "link"]) {
                        Ok(text) => parse_iw_link(&text, &mut wifi),
                        Err(e) => wifi.state = Some(format!("link not read: {e}")),
                    }
                    wifi
                })
                .collect()
        });
        NetworkContext {
            adapters,
            gateways,
            dns_servers: resolv.clone().map(|(s, _)| s),
            search_domains: resolv.map(|(_, d)| d),
            wifi,
        }
    }

    #[cfg(target_os = "macos")]
    {
        let adapters = capture("ifconfig", &[]).map(|t| parse_ifconfig(&t));
        let arp = capture("arp", &["-an"]).unwrap_or_default();
        let gateways = capture("netstat", &["-rn"]).map(|t| parse_netstat_defaults(&t, &arp));
        let dns = capture("scutil", &["--dns"]).map(|t| parse_scutil_dns(&t));
        let wifi = capture("system_profiler", &["SPAirPortDataType", "-json"]).and_then(|t| {
            serde_json::from_str(&t)
                .map(|j| parse_airport(&j))
                .map_err(|e| format!("system_profiler SPAirPortDataType is not JSON: {e}"))
        });
        NetworkContext {
            adapters,
            gateways,
            dns_servers: dns.clone().map(|(s, _)| s),
            search_domains: dns.map(|(_, d)| d),
            wifi,
        }
    }

    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        let none = || Err("no network context reader for this platform".to_string());
        NetworkContext {
            adapters: none(),
            gateways: none(),
            dns_servers: none(),
            search_domains: none(),
            wifi: none(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macs_are_normalised_and_blank_ones_are_none() {
        assert_eq!(
            normalize_mac("84-9E-56-FA-D6-3E").as_deref(),
            Some("84:9e:56:fa:d6:3e")
        );
        assert_eq!(
            normalize_mac("84:9e:56:fa:d6:3d").as_deref(),
            Some("84:9e:56:fa:d6:3d")
        );
        assert_eq!(normalize_mac(""), None);
        assert_eq!(normalize_mac("00-00-00-00-00-00"), None);
        assert_eq!(normalize_mac("not a mac"), None);
    }

    /// A trimmed capture of `netsh wlan show interfaces` on Windows 11.
    const NETSH_CONNECTED: &str = "
There is 1 interface on the system:

    Name                   : Wi-Fi
    Description            : Intel(R) Wi-Fi 6E AX211 160MHz
    GUID                   : 0d7c-example
    Physical address       : a4:6b:b6:12:34:56
    Interface type         : Primary
    State                  : connected
    SSID                   : HomeNet
    AP BSSID               : 7c:10:c9:aa:bb:cc
    Band                   : 5 GHz
    Channel                : 44
    Network type           : Infrastructure
    Radio type             : 802.11ax
    Authentication         : WPA3-Personal
    Cipher                 : CCMP
    Connection mode        : Auto Connect
    Receive rate (Mbps)    : 1201
    Transmit rate (Mbps)   : 960.5
    Signal                 : 92%
    Rssi                   : -48
    Profile                : HomeNet

    Hosted network status  : Not available
";

    #[test]
    fn netsh_reports_the_connection_it_printed() {
        let wifi = parse_netsh_wlan(NETSH_CONNECTED).unwrap();
        assert_eq!(wifi.len(), 1);
        let w = &wifi[0];
        assert_eq!(w.interface, "Wi-Fi");
        assert_eq!(w.connected, Some(true));
        assert_eq!(w.ssid.as_deref(), Some("HomeNet"));
        assert_eq!(w.bssid.as_deref(), Some("7c:10:c9:aa:bb:cc"));
        assert_eq!(w.mac.as_deref(), Some("a4:6b:b6:12:34:56"));
        assert_eq!(w.band.as_deref(), Some("5 GHz"));
        assert_eq!(w.channel, Some(44));
        assert_eq!(w.radio_type.as_deref(), Some("802.11ax"));
        assert_eq!(w.authentication.as_deref(), Some("WPA3-Personal"));
        assert_eq!(w.cipher.as_deref(), Some("CCMP"));
        assert_eq!(w.receive_rate_mbps, Some(1201.0));
        assert_eq!(w.transmit_rate_mbps, Some(960.5));
        assert_eq!(w.signal_percent, Some(92));
        assert_eq!(w.rssi_dbm, Some(-48));
        assert_eq!(w.profile.as_deref(), Some("HomeNet"));
    }

    /// This machine's own output: a radio that is switched off.
    #[test]
    fn a_disconnected_interface_has_no_connection_fields() {
        let text = "There is 1 interface on the system: \n\n    Name                   : Wi-Fi\n    Description            : MediaTek Wi-Fi 7 MT7927 Wireless LAN Card\n    Physical address       : 84:9e:56:fa:d6:3d\n    State                  : disconnected\n    Radio status           : Hardware On\n                             Software Off\n";
        let wifi = parse_netsh_wlan(text).unwrap();
        assert_eq!(wifi.len(), 1);
        assert_eq!(wifi[0].connected, Some(false));
        assert_eq!(wifi[0].state.as_deref(), Some("disconnected"));
        assert_eq!(wifi[0].ssid, None);
        assert_eq!(wifi[0].signal_percent, None);
    }

    /// `netsh` labels are English-only. The first fixture names its interface
    /// under a label this reader does not know, so nothing is recognised: that
    /// is an error, not an empty list. The label is an assumed localisation,
    /// not one captured from a German Windows.
    #[test]
    fn localised_netsh_output_with_no_known_label_is_an_error_not_no_wifi() {
        let text = concat!(
            "Es ist 1 Schnittstelle auf dem System vorhanden:\n\n",
            "    Schnittstellenname     : WLAN\n",
            "    Beschreibung           : Intel(R) Wi-Fi 6E AX211 160MHz\n",
            "    Status                 : verbunden\n",
            "    SSID                   : HomeNet\n",
        );
        assert!(parse_netsh_wlan(text).is_err());
    }

    /// A German listing whose interface line is still `Name` is read for that
    /// interface. Its other labels are not English, so they stay unknown. The
    /// connection state is not inferred from the German `Status` line.
    #[test]
    fn localised_netsh_fields_are_unknown_not_guessed() {
        let text = concat!(
            "Es ist 1 Schnittstelle auf dem System vorhanden:\n\n",
            "    Name                   : WLAN\n",
            "    Beschreibung           : Intel(R) Wi-Fi 6E AX211 160MHz\n",
            "    Physische Adresse      : a4:6b:b6:12:34:56\n",
            "    Status                 : verbunden\n",
            "    SSID                   : HomeNet\n",
        );
        let wifi = parse_netsh_wlan(text).unwrap();
        assert_eq!(wifi.len(), 1);
        assert_eq!(wifi[0].interface, "WLAN");
        assert_eq!(wifi[0].connected, None);
        assert_eq!(wifi[0].state, None);
        assert_eq!(wifi[0].mac, None);
        // `SSID` is the same word in German, so it is still read.
        assert_eq!(wifi[0].ssid.as_deref(), Some("HomeNet"));
    }

    #[test]
    fn a_gateway_mac_comes_only_from_the_arp_row_for_its_own_interface() {
        let netstat = "default 192.168.1.1 UGScg en0\n";
        let own = "? (192.168.1.1) at f4:92:bf:01:02:03 on en0 ifscope [ethernet]\n";
        let other = "? (192.168.1.1) at f4:92:bf:01:02:03 on en1 ifscope [ethernet]\n";
        // A row with no `on <device>` is not attached to any interface.
        let no_device = "? (192.168.1.1) at f4:92:bf:01:02:03 ifscope [ethernet]\n";
        assert_eq!(
            parse_netstat_defaults(netstat, own)[0].mac.as_deref(),
            Some("f4:92:bf:01:02:03")
        );
        assert_eq!(parse_netstat_defaults(netstat, other)[0].mac, None);
        assert_eq!(parse_netstat_defaults(netstat, no_device)[0].mac, None);
    }

    /// Windows reports a link speed for each direction. One figure is taken
    /// only when both agree; a link whose two ends differ has no single speed.
    #[test]
    fn link_speed_is_taken_only_when_both_directions_agree() {
        let json: serde_json::Value = serde_json::from_str(
            r#"{
            "adapters":[
                {"Name":"Ethernet","ifIndex":19,"Status":"Up","MtuSize":1500,
                 "ReceiveLinkSpeed":1000000000,"TransmitLinkSpeed":2500000000},
                {"Name":"Wi-Fi","ifIndex":3,"Status":"Up","MtuSize":1500,
                 "ReceiveLinkSpeed":1000000000,"TransmitLinkSpeed":1000000000}
            ]
        }"#,
        )
        .unwrap();
        let (adapters, _, _, _) = parse_windows(&json);
        assert_eq!(adapters[0].link_speed_bps, None);
        assert_eq!(adapters[1].link_speed_bps, Some(1_000_000_000));
    }

    #[test]
    fn netsh_location_refusal_is_an_error_not_no_wifi() {
        let text = "Network shell commands need location permission to access WLAN information.";
        assert!(parse_netsh_wlan(text).unwrap_err().contains("location"));
    }

    #[test]
    fn windows_query_output_joins_on_interface_index() {
        let json: serde_json::Value = serde_json::from_str(r#"{
            "adapters":[
                {"Name":"Ethernet","InterfaceDescription":"Realtek PCIe 2.5GbE","ifIndex":19,"Status":"Up",
                 "MacAddress":"9C-6B-00-11-22-33","MtuSize":1500,"ReceiveLinkSpeed":2500000000,
                 "TransmitLinkSpeed":2500000000,"MediaType":"802.3","Virtual":false,
                 "DriverProvider":"Realtek","DriverVersion":"10.68.0","DriverDate":"2024-03-01"},
                {"Name":"Wi-Fi 4","ifIndex":27,"Status":"Not Present","MacAddress":"","MtuSize":0,
                 "ReceiveLinkSpeed":0,"Virtual":false}
            ],
            "addresses":[
                {"InterfaceIndex":19,"IPAddress":"10.0.0.42","PrefixLength":24,"AddressFamily":"IPv4"},
                {"InterfaceIndex":19,"IPAddress":"fe80::1%19","PrefixLength":64,"AddressFamily":"IPv6"}
            ],
            "routes":{"InterfaceIndex":19,"InterfaceAlias":"Ethernet","NextHop":"10.0.0.1","RouteMetric":25,"AddressFamily":"IPv4"},
            "neighbors":[{"InterfaceIndex":19,"IPAddress":"10.0.0.1","LinkLayerAddress":"F4-92-BF-01-02-03","State":"Reachable"}],
            "dns":[{"InterfaceIndex":19,"ServerAddresses":["10.0.0.1","1.1.1.1"]},
                   {"InterfaceIndex":27,"ServerAddresses":["fec0:0:0:ffff::1"]}],
            "suffixes":[{"InterfaceIndex":19,"ConnectionSpecificSuffix":"lan"}],
            "search":["corp.example"]
        }"#).unwrap();
        let (adapters, gateways, dns, search) = parse_windows(&json);
        assert_eq!(adapters.len(), 2);
        let eth = &adapters[0];
        assert_eq!(eth.mac.as_deref(), Some("9c:6b:00:11:22:33"));
        assert_eq!(eth.link_speed_bps, Some(2_500_000_000));
        assert_eq!(eth.addresses.len(), 2);
        assert_eq!(eth.addresses[1].address, "fe80::1");
        assert_eq!(eth.addresses[1].family, "ipv6");
        assert_eq!(eth.dns_servers, vec!["10.0.0.1", "1.1.1.1"]);
        assert_eq!(eth.dns_suffix.as_deref(), Some("lan"));
        // An absent adapter has no address, MTU or link rate, not zeros.
        assert_eq!(adapters[1].mac, None);
        assert_eq!(adapters[1].mtu, None);
        assert_eq!(adapters[1].link_speed_bps, None);
        assert_eq!(gateways.len(), 1);
        assert_eq!(gateways[0].address, "10.0.0.1");
        assert_eq!(gateways[0].mac.as_deref(), Some("f4:92:bf:01:02:03"));
        assert_eq!(gateways[0].metric, Some(25));
        // The absent adapter's placeholder server is its own, not the machine's.
        assert_eq!(adapters[1].dns_servers, vec!["fec0:0:0:ffff::1"]);
        assert_eq!(dns, vec!["10.0.0.1", "1.1.1.1"]);
        assert_eq!(search, vec!["corp.example"]);
    }

    #[test]
    fn linux_ip_json_and_iw_are_read() {
        let addr: serde_json::Value = serde_json::from_str(
            r#"[
            {"ifindex":2,"ifname":"eth0","mtu":1500,"operstate":"UP","link_type":"ether",
             "address":"52:54:00:12:34:56","addr_info":[
                {"family":"inet","local":"192.168.1.20","prefixlen":24},
                {"family":"inet6","local":"fe80::5054:ff:fe12:3456","prefixlen":64}]}
        ]"#,
        )
        .unwrap();
        let adapters = parse_ip_addr(&addr);
        assert_eq!(adapters[0].mac.as_deref(), Some("52:54:00:12:34:56"));
        assert_eq!(adapters[0].addresses[0].prefix_length, Some(24));

        let routes: serde_json::Value = serde_json::from_str(
            r#"[{"dst":"default","gateway":"192.168.1.1","dev":"eth0","protocol":"dhcp","metric":100}]"#,
        ).unwrap();
        let neigh: serde_json::Value = serde_json::from_str(
            r#"[{"dst":"192.168.1.1","dev":"eth0","lladdr":"f4:92:bf:01:02:03","state":["REACHABLE"]}]"#,
        ).unwrap();
        let gw = parse_ip_routes(&routes, &neigh);
        assert_eq!(gw[0].mac.as_deref(), Some("f4:92:bf:01:02:03"));
        assert_eq!(gw[0].metric, Some(100));

        let (servers, search) =
            parse_resolv_conf("# comment\nnameserver 127.0.0.53\nsearch lan example.com\n");
        assert_eq!(servers, vec!["127.0.0.53"]);
        assert_eq!(search, vec!["lan", "example.com"]);

        let devs = parse_iw_dev("phy#0\n\tInterface wlan0\n\t\tifindex 3\n\t\taddr 3c:22:fb:aa:bb:cc\n\t\ttype managed\n");
        assert_eq!(
            devs,
            vec![("wlan0".to_string(), Some("3c:22:fb:aa:bb:cc".to_string()))]
        );
        let mut wifi = WifiInterface::default();
        parse_iw_link(
            "Connected to 7c:10:c9:aa:bb:cc (on wlan0)\n\tSSID: HomeNet\n\tfreq: 5220\n\tRX: 1 bytes\n\tsignal: -54 dBm\n\trx bitrate: 866.7 MBit/s VHT-MCS 9\n\ttx bitrate: 650.0 MBit/s\n",
            &mut wifi,
        );
        assert_eq!(wifi.connected, Some(true));
        assert_eq!(wifi.ssid.as_deref(), Some("HomeNet"));
        assert_eq!(wifi.frequency_mhz, Some(5220));
        assert_eq!(wifi.channel, Some(44));
        assert_eq!(wifi.band.as_deref(), Some("5 GHz"));
        assert_eq!(wifi.rssi_dbm, Some(-54));
        assert_eq!(wifi.receive_rate_mbps, Some(866.7));
        let mut idle = WifiInterface::default();
        parse_iw_link("Not connected.\n", &mut idle);
        assert_eq!(idle.connected, Some(false));
        assert_eq!(idle.ssid, None);
    }

    #[test]
    fn channels_follow_the_standard() {
        assert_eq!(channel_of(2412), Some(1));
        assert_eq!(channel_of(2484), Some(14));
        assert_eq!(channel_of(5180), Some(36));
        assert_eq!(channel_of(5955), Some(1));
        assert_eq!(band_of(6115), Some("6 GHz"));
        assert_eq!(channel_of(60480), None);
    }

    #[test]
    fn macos_tools_are_read() {
        let ifconfig = "lo0: flags=8049<UP,LOOPBACK,RUNNING,MULTICAST> mtu 16384\n\tinet 127.0.0.1 netmask 0xff000000\nen0: flags=8863<UP,BROADCAST,SMART,RUNNING,SIMPLEX,MULTICAST> mtu 1500\n\tether 14:7d:da:aa:bb:cc\n\tinet6 fe80::1c2b:aaaa:bbbb:cccc%en0 prefixlen 64 secured scopeid 0xe\n\tinet 192.168.1.23 netmask 0xffffff00 broadcast 192.168.1.255\n\tmedia: autoselect\n\tstatus: active\n";
        let adapters = parse_ifconfig(ifconfig);
        assert_eq!(adapters.len(), 2);
        assert_eq!(adapters[1].name, "en0");
        assert_eq!(adapters[1].mtu, Some(1500));
        assert_eq!(adapters[1].mac.as_deref(), Some("14:7d:da:aa:bb:cc"));
        assert_eq!(adapters[1].status.as_deref(), Some("active"));
        assert_eq!(adapters[1].addresses[1].prefix_length, Some(24));
        assert_eq!(
            adapters[1].addresses[0].address,
            "fe80::1c2b:aaaa:bbbb:cccc"
        );

        let netstat = concat!(
            "Routing tables\n\nInternet:\n",
            "Destination Gateway Flags Netif Expire\n",
            "default 192.168.1.1 UGScg en0\n",
            "127 127.0.0.1 UCS lo0\n\n",
            "Internet6:\n",
            "Destination Gateway Flags Netif Expire\n",
            "default fe80::1%en0 UGcg en0\n",
        );
        let arp = "? (192.168.1.1) at f4:92:bf:1:2:3 on en0 ifscope [ethernet]\n";
        let gw = parse_netstat_defaults(netstat, arp);
        assert_eq!(gw.len(), 2);
        assert_eq!(gw[0].address, "192.168.1.1");
        assert_eq!(gw[0].interface.as_deref(), Some("en0"));
        assert_eq!(gw[1].address, "fe80::1");
        assert_eq!(gw[1].family, "ipv6");

        let (servers, search) = parse_scutil_dns("DNS configuration\n\nresolver #1\n  search domain[0] : lan\n  nameserver[0] : 192.168.1.1\n  nameserver[1] : 1.1.1.1\n\nresolver #2\n  nameserver[0] : 192.168.1.1\n");
        assert_eq!(servers, vec!["192.168.1.1", "1.1.1.1"]);
        assert_eq!(search, vec!["lan"]);

        let airport: serde_json::Value = serde_json::from_str(r#"{"SPAirPortDataType":[{"spairport_airport_interfaces":[
            {"_name":"en0","spairport_status_information":"spairport_status_connected",
             "spairport_wireless_card_type":"Wi-Fi (0x14E4, 0x4387)","spairport_wireless_mac_address":"14:7d:da:aa:bb:cc",
             "spairport_current_network_information":{"_name":"<redacted>","spairport_network_channel":"36 (5GHz, 80MHz)",
               "spairport_network_phymode":"802.11ax","spairport_network_rate":864,
               "spairport_security_mode":"spairport_security_mode_wpa3_transition",
               "spairport_signal_noise":"-51 dBm / -93 dBm","spairport_network_country_code":"US"}},
            {"_name":"awdl0"}]}]}"#).unwrap();
        let wifi = parse_airport(&airport);
        assert_eq!(wifi.len(), 2);
        let w = &wifi[0];
        assert_eq!(w.ssid, None, "a redacted SSID is not a name");
        assert_eq!(w.channel, Some(36));
        assert_eq!(w.band.as_deref(), Some("5 GHz"));
        assert_eq!(w.channel_width_mhz, Some(80));
        assert_eq!(w.rssi_dbm, Some(-51));
        assert_eq!(w.noise_dbm, Some(-93));
        assert_eq!(w.authentication.as_deref(), Some("wpa3 transition"));
        assert_eq!(w.state.as_deref(), Some("connected"));
        assert_eq!(wifi[1].connected, Some(false));
    }
}
