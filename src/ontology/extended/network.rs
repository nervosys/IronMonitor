//! Interface addresses and state, default gateways, DNS servers, and the Wi-Fi
//! connection, read by [`crate::net_context`].
//!
//! The base resolver's `network.{iface}.*` rows are traffic counters from
//! `network_monitor`. These extend the same prefix with what the adapter is and
//! where it is attached, and add the gateway, DNS and Wi-Fi rows nothing in the
//! base schema had a reader for.

use super::super::resolve::*;
use super::super::{Domain as D, Entity, EntityKind as K, Provenance as P, Unit as U};
use crate::net_context::{self, NetworkContext, WifiInterface};
use serde_json::json;

pub(super) fn declare(add: &mut dyn FnMut(Entity)) {
    for (id, desc) in [
        (
            "network.adapter.<none>",
            "Present when no adapter's addresses could be read, carrying why.",
        ),
        (
            "network.gateway.<none>",
            "Present when this machine has no default route, or the route table could not be read.",
        ),
        (
            "network.wifi.<none>",
            "Present when no Wi-Fi interface was enumerated, or Wi-Fi could not be queried, carrying why.",
        ),
    ] {
        add(Entity::new(id, D::Network, K::Diagnostic, None, P::Unavailable, true, desc));
    }
    let mut e = |id: &'static str, kind: K, unit: Option<U>, nullable: bool, desc: &'static str| {
        add(Entity::new(
            id,
            D::Network,
            kind,
            unit,
            P::Measured,
            nullable,
            desc,
        ))
    };

    // ── Per adapter ──────────────────────────────────────────────────────────
    e(
        "network.{iface}.description",
        K::Identity,
        Some(U::Text),
        true,
        "The driver's description of the adapter hardware, e.g. its chipset name.",
    );
    e(
        "network.{iface}.status",
        K::Measurement,
        Some(U::Identifier),
        true,
        "Operational state as the OS words it: Up, Disconnected, Not Present, UP, active.",
    );
    e(
        "network.{iface}.mac",
        K::Identity,
        Some(U::Identifier),
        true,
        "Link-layer (MAC) address, colon-separated lowercase hex.",
    );
    e(
        "network.{iface}.mtu",
        K::Measurement,
        Some(U::Bytes),
        true,
        "Maximum transmission unit configured on the adapter.",
    );
    e("network.{iface}.ipv4", K::Measurement, None, false,
      "IPv4 addresses assigned to the adapter, each as address/prefix. An empty list is a reading: none assigned.");
    e(
        "network.{iface}.ipv6",
        K::Measurement,
        None,
        false,
        "IPv6 addresses assigned to the adapter, each as address/prefix, zone suffix removed.",
    );
    e(
        "network.{iface}.media_type",
        K::Identity,
        Some(U::Text),
        true,
        "Media or link type the OS reports: 802.3, Native 802.11, ether, autoselect.",
    );
    e("network.{iface}.virtual", K::Identity, None, true,
      "Whether the OS classes the adapter as virtual (a VPN tunnel, a hypervisor switch). Windows only.");
    e(
        "network.{iface}.driver",
        K::Identity,
        Some(U::Text),
        true,
        "The kernel driver bound to the adapter (Linux) or the driver's provider (Windows).",
    );
    e(
        "network.{iface}.driver_version",
        K::Identity,
        Some(U::Identifier),
        true,
        "Driver version the OS reports for the adapter.",
    );
    e(
        "network.{iface}.driver_date",
        K::Identity,
        Some(U::Text),
        true,
        "Driver date the OS reports for the adapter.",
    );
    e("network.{iface}.dns_servers", K::Measurement, None, true,
      "DNS servers configured on this adapter. Windows configures DNS per adapter; elsewhere see network.dns.servers.");
    e(
        "network.{iface}.dns_suffix",
        K::Measurement,
        Some(U::Text),
        true,
        "Connection-specific DNS suffix configured on the adapter (Windows).",
    );

    // ── Gateways ─────────────────────────────────────────────────────────────
    e(
        "network.gateway.{n}.address",
        K::Measurement,
        Some(U::Identifier),
        false,
        "Next-hop address of a default route: the local router or gateway.",
    );
    e(
        "network.gateway.{n}.family",
        K::Identity,
        Some(U::Identifier),
        false,
        "Address family of the route: ipv4 or ipv6.",
    );
    e(
        "network.gateway.{n}.interface",
        K::Measurement,
        Some(U::Text),
        true,
        "The interface the default route leaves by.",
    );
    e(
        "network.gateway.{n}.metric",
        K::Measurement,
        Some(U::Count),
        true,
        "Route metric; lower is preferred where several default routes exist.",
    );
    e("network.gateway.{n}.mac", K::Measurement, Some(U::Identifier), true,
      "The gateway's link-layer address, from the neighbour (ARP/NDP) table. Absent for a VPN's gateway or one not yet resolved.");

    // ── DNS ──────────────────────────────────────────────────────────────────
    e(
        "network.dns.servers",
        K::Measurement,
        None,
        false,
        "DNS servers in resolution order across the interfaces that are up, de-duplicated.",
    );
    e(
        "network.dns.search_domains",
        K::Measurement,
        None,
        false,
        "DNS search domains. An empty list is a reading: none configured.",
    );

    // ── Wi-Fi ────────────────────────────────────────────────────────────────
    e(
        "network.wifi.{n}.interface",
        K::Identity,
        Some(U::Text),
        false,
        "The OS's name for the wireless interface: Wi-Fi, wlan0, en0.",
    );
    e(
        "network.wifi.{n}.description",
        K::Identity,
        Some(U::Text),
        true,
        "The wireless card as the OS describes it.",
    );
    e(
        "network.wifi.{n}.mac",
        K::Identity,
        Some(U::Identifier),
        true,
        "The wireless interface's own MAC address.",
    );
    e(
        "network.wifi.{n}.state",
        K::Measurement,
        Some(U::Identifier),
        true,
        "Interface state as the tool words it: connected, disconnected, not connected.",
    );
    e(
        "network.wifi.{n}.connected",
        K::Measurement,
        None,
        true,
        "Whether the interface is associated with an access point.",
    );
    e(
        "network.wifi.{n}.ssid",
        K::Measurement,
        Some(U::Text),
        true,
        "Name of the network the interface is connected to.",
    );
    e(
        "network.wifi.{n}.bssid",
        K::Measurement,
        Some(U::Identifier),
        true,
        "MAC address of the access point radio the interface is associated with.",
    );
    e(
        "network.wifi.{n}.radio_type",
        K::Measurement,
        Some(U::Identifier),
        true,
        "PHY standard of the connection, e.g. 802.11ax.",
    );
    e(
        "network.wifi.{n}.authentication",
        K::Measurement,
        Some(U::Text),
        true,
        "Security mode of the connection, e.g. WPA3-Personal.",
    );
    e(
        "network.wifi.{n}.cipher",
        K::Measurement,
        Some(U::Identifier),
        true,
        "Cipher protecting the connection, e.g. CCMP.",
    );
    e(
        "network.wifi.{n}.band",
        K::Measurement,
        Some(U::Text),
        true,
        "Frequency band of the connection: 2.4 GHz, 5 GHz or 6 GHz.",
    );
    e(
        "network.wifi.{n}.channel",
        K::Measurement,
        Some(U::Count),
        true,
        "IEEE channel number of the connection.",
    );
    e("network.wifi.{n}.frequency", K::Measurement, Some(U::Megahertz), true,
      "Centre frequency of the connection's channel (Linux reports it; elsewhere only the channel is printed).");
    e(
        "network.wifi.{n}.channel_width",
        K::Measurement,
        Some(U::Megahertz),
        true,
        "Channel width of the connection (macOS reports it).",
    );
    e(
        "network.wifi.{n}.signal",
        K::Measurement,
        Some(U::Percent),
        true,
        "Signal quality as a percentage, as Windows reports it. Not a dBm figure.",
    );
    e(
        "network.wifi.{n}.rssi",
        K::Measurement,
        Some(U::Dbm),
        true,
        "Received signal strength.",
    );
    e(
        "network.wifi.{n}.noise",
        K::Measurement,
        Some(U::Dbm),
        true,
        "Noise floor on the channel (macOS reports it).",
    );
    e(
        "network.wifi.{n}.rx_rate",
        K::Measurement,
        Some(U::MegabitsPerSecond),
        true,
        "Negotiated receive link rate. A PHY rate, not throughput.",
    );
    e(
        "network.wifi.{n}.tx_rate",
        K::Measurement,
        Some(U::MegabitsPerSecond),
        true,
        "Negotiated transmit link rate. A PHY rate, not throughput.",
    );
    e(
        "network.wifi.{n}.profile",
        K::Measurement,
        Some(U::Text),
        true,
        "The saved profile the connection uses (Windows).",
    );
    e(
        "network.wifi.{n}.country_code",
        K::Measurement,
        Some(U::Identifier),
        true,
        "Regulatory country code the access point advertises (macOS reports it).",
    );
}

pub(super) fn resolve(out: &mut Vec<Reading>) {
    let ctx = net_context::read();
    resolve_adapters(out, &ctx);
    resolve_gateways(out, &ctx);
    resolve_dns(out, &ctx);
    resolve_wifi(out, &ctx);
}

fn text_or(out: &mut Vec<Reading>, id: String, value: Option<&str>, unit: U, why: &str) {
    match value {
        Some(v) => push_str_as(out, id, v, unit, P::Measured),
        None => out.push(Reading::unavailable(id, Some(unit), why)),
    }
}

fn resolve_adapters(out: &mut Vec<Reading>, ctx: &NetworkContext) {
    let adapters = match &ctx.adapters {
        Ok(a) if a.is_empty() => {
            out.push(Reading::unavailable(
                "network.adapter.<none>",
                None,
                "no network adapters enumerated",
            ));
            return;
        }
        Ok(a) => a,
        Err(e) => {
            out.push(Reading::unavailable(
                "network.adapter.<none>",
                None,
                format!("adapter addresses could not be read: {e}"),
            ));
            return;
        }
    };
    let unreported = "the OS did not report it for this adapter";
    for a in adapters {
        let base = format!("network.{}", id_segment(&a.name));
        text_or(
            out,
            format!("{base}.description"),
            a.description.as_deref(),
            U::Text,
            unreported,
        );
        text_or(
            out,
            format!("{base}.status"),
            a.status.as_deref(),
            U::Identifier,
            unreported,
        );
        text_or(
            out,
            format!("{base}.mac"),
            a.mac.as_deref(),
            U::Identifier,
            "the adapter reports no link-layer address (a tunnel, or a device not present)",
        );
        push_opt(
            out,
            format!("{base}.mtu"),
            a.mtu.map(|m| json!(m)),
            Some(U::Bytes),
            unreported,
        );
        for family in ["ipv4", "ipv6"] {
            let list: Vec<String> = a
                .addresses
                .iter()
                .filter(|ip| ip.family == family)
                .map(|ip| match ip.prefix_length {
                    Some(p) => format!("{}/{p}", ip.address),
                    None => ip.address.clone(),
                })
                .collect();
            out.push(Reading::measured(
                format!("{base}.{family}"),
                json!(list),
                None,
            ));
        }
        text_or(
            out,
            format!("{base}.media_type"),
            a.media_type.as_deref(),
            U::Text,
            unreported,
        );
        push_opt(
            out,
            format!("{base}.virtual"),
            a.is_virtual.map(|v| json!(v)),
            None,
            "only Windows classes adapters as virtual",
        );
        text_or(
            out,
            format!("{base}.driver"),
            a.driver.as_deref(),
            U::Text,
            unreported,
        );
        text_or(
            out,
            format!("{base}.driver_version"),
            a.driver_version.as_deref(),
            U::Identifier,
            unreported,
        );
        text_or(
            out,
            format!("{base}.driver_date"),
            a.driver_date.as_deref(),
            U::Text,
            unreported,
        );
        if cfg!(windows) {
            out.push(Reading::measured(
                format!("{base}.dns_servers"),
                json!(a.dns_servers),
                None,
            ));
        } else {
            out.push(Reading::unavailable(
                format!("{base}.dns_servers"),
                None,
                "DNS is configured globally on this platform; see network.dns.servers",
            ));
        }
        text_or(
            out,
            format!("{base}.dns_suffix"),
            a.dns_suffix.as_deref(),
            U::Text,
            if cfg!(windows) {
                "no connection-specific suffix is configured on this adapter"
            } else {
                "connection-specific DNS suffixes are a Windows setting"
            },
        );
    }
}

fn resolve_gateways(out: &mut Vec<Reading>, ctx: &NetworkContext) {
    let gateways = match &ctx.gateways {
        Ok(g) if g.is_empty() => {
            out.push(Reading::unavailable(
                "network.gateway.<none>",
                None,
                "no default route is configured on this machine",
            ));
            return;
        }
        Ok(g) => g,
        Err(e) => {
            out.push(Reading::unavailable(
                "network.gateway.<none>",
                None,
                format!("the route table could not be read: {e}"),
            ));
            return;
        }
    };
    for (n, g) in gateways.iter().enumerate() {
        let base = format!("network.gateway.{n}");
        push_id(out, format!("{base}.address"), &g.address);
        push_id(out, format!("{base}.family"), &g.family);
        text_or(
            out,
            format!("{base}.interface"),
            g.interface.as_deref(),
            U::Text,
            "the route table did not name the interface",
        );
        push_opt(
            out,
            format!("{base}.metric"),
            g.metric.map(|m| json!(m)),
            Some(U::Count),
            "the route table on this platform does not print metrics",
        );
        text_or(
            out,
            format!("{base}.mac"),
            g.mac.as_deref(),
            U::Identifier,
            "the neighbour table has no resolved entry for this gateway (a VPN's gateway has none)",
        );
    }
}

fn resolve_dns(out: &mut Vec<Reading>, ctx: &NetworkContext) {
    for (id, list) in [
        ("network.dns.servers", &ctx.dns_servers),
        ("network.dns.search_domains", &ctx.search_domains),
    ] {
        match list {
            Ok(l) => out.push(Reading::measured(id, json!(l), None)),
            Err(e) => out.push(Reading::unavailable(
                id,
                None,
                format!("DNS configuration could not be read: {e}"),
            )),
        }
    }
}

fn resolve_wifi(out: &mut Vec<Reading>, ctx: &NetworkContext) {
    let wifi = match &ctx.wifi {
        Ok(w) if w.is_empty() => {
            out.push(Reading::unavailable(
                "network.wifi.<none>",
                None,
                "no Wi-Fi interface enumerated on this machine",
            ));
            return;
        }
        Ok(w) => w,
        Err(e) => {
            out.push(Reading::unavailable("network.wifi.<none>", None, e.clone()));
            return;
        }
    };
    for (n, w) in wifi.iter().enumerate() {
        resolve_one_wifi(out, &format!("network.wifi.{n}"), w);
    }
}

fn resolve_one_wifi(out: &mut Vec<Reading>, base: &str, w: &WifiInterface) {
    let unreported = "the tool did not report it";
    push_text(out, format!("{base}.interface"), &w.interface);
    text_or(
        out,
        format!("{base}.description"),
        w.description.as_deref(),
        U::Text,
        unreported,
    );
    text_or(
        out,
        format!("{base}.mac"),
        w.mac.as_deref(),
        U::Identifier,
        unreported,
    );
    text_or(
        out,
        format!("{base}.state"),
        w.state.as_deref(),
        U::Identifier,
        unreported,
    );
    push_opt(
        out,
        format!("{base}.connected"),
        w.connected.map(|c| json!(c)),
        None,
        unreported,
    );

    // Connection fields mean nothing without a connection. Saying so is more
    // useful than a row of "not reported".
    let why = if w.connected == Some(true) {
        unreported
    } else {
        "not connected to a network"
    };
    let text = |out: &mut Vec<Reading>, field: &str, value: &Option<String>, unit: U| {
        text_or(out, format!("{base}.{field}"), value.as_deref(), unit, why)
    };
    text(out, "ssid", &w.ssid, U::Text);
    text(out, "bssid", &w.bssid, U::Identifier);
    text(out, "radio_type", &w.radio_type, U::Identifier);
    text(out, "authentication", &w.authentication, U::Text);
    text(out, "cipher", &w.cipher, U::Identifier);
    text(out, "band", &w.band, U::Text);
    let num = |out: &mut Vec<Reading>, field: &str, value: Option<serde_json::Value>, unit: U| {
        push_opt(out, format!("{base}.{field}"), value, Some(unit), why)
    };
    num(out, "channel", w.channel.map(|v| json!(v)), U::Count);
    num(
        out,
        "frequency",
        w.frequency_mhz.map(|v| json!(v)),
        U::Megahertz,
    );
    num(
        out,
        "channel_width",
        w.channel_width_mhz.map(|v| json!(v)),
        U::Megahertz,
    );
    num(
        out,
        "signal",
        w.signal_percent.map(|v| json!(v)),
        U::Percent,
    );
    num(out, "rssi", w.rssi_dbm.map(|v| json!(v)), U::Dbm);
    num(out, "noise", w.noise_dbm.map(|v| json!(v)), U::Dbm);
    num(
        out,
        "rx_rate",
        w.receive_rate_mbps.map(|v| json!(v)),
        U::MegabitsPerSecond,
    );
    num(
        out,
        "tx_rate",
        w.transmit_rate_mbps.map(|v| json!(v)),
        U::MegabitsPerSecond,
    );
    text(out, "profile", &w.profile, U::Text);
    text(out, "country_code", &w.country_code, U::Identifier);
}
