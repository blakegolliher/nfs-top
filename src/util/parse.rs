use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

pub fn parse_kv_options(line: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for tok in line.split([',', ' ']) {
        let t = tok.trim();
        if t.is_empty() {
            continue;
        }
        if let Some((k, v)) = t.split_once('=') {
            map.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    map
}

pub fn parse_ip_maybe(s: &str) -> Option<IpAddr> {
    s.parse::<IpAddr>().ok()
}

pub fn parse_tcp_hex_endpoint(addr_hex: &str, port_hex: &str, v6: bool) -> Option<(IpAddr, u16)> {
    let port = u16::from_str_radix(port_hex, 16).ok()?;
    let ip = if v6 {
        if addr_hex.len() != 32 {
            return None;
        }
        // Linux /proc/net/tcp6 prints each s6_addr32[i] with %08X (native u32).
        // Parse each 4-byte group and convert back via to_ne_bytes() to recover
        // the original network-order bytes.
        let mut bytes = [0u8; 16];
        for g in 0..4 {
            let off = g * 8;
            let word = u32::from_str_radix(&addr_hex[off..off + 8], 16).ok()?;
            bytes[g * 4..g * 4 + 4].copy_from_slice(&word.to_ne_bytes());
        }
        IpAddr::V6(Ipv6Addr::from(bytes))
    } else {
        if addr_hex.len() != 8 {
            return None;
        }
        let raw = u32::from_str_radix(addr_hex, 16).ok()?;
        let b = raw.to_le_bytes();
        IpAddr::V4(Ipv4Addr::new(b[0], b[1], b[2], b[3]))
    };
    Some((ip, port))
}
