use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::net::IpAddr;

use crate::util::parse::parse_tcp_hex_endpoint;

#[derive(Debug, Clone, Default)]
pub struct SocketObs {
    pub by_remote_ip: HashMap<IpAddr, u64>,
    pub raw_matches: Vec<String>,
}

pub fn read_observed_nfs(remote_ports: &[u16]) -> Result<SocketObs> {
    let mut out = SocketObs::default();
    let v4 = fs::read_to_string("/proc/net/tcp").context("reading /proc/net/tcp")?;
    parse_tcp_lines(&v4, false, remote_ports, &mut out);
    let v6 = fs::read_to_string("/proc/net/tcp6").context("reading /proc/net/tcp6")?;
    parse_tcp_lines(&v6, true, remote_ports, &mut out);
    Ok(out)
}

pub fn parse_tcp_lines(input: &str, v6: bool, ports: &[u16], out: &mut SocketObs) {
    for line in input.lines().skip(1) {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 10 {
            continue;
        }
        let (raddr, rport) = match cols[2].split_once(':') {
            Some(x) => x,
            None => continue,
        };
        let state = cols[3];
        if state != "01" {
            continue;
        }
        let Some((ip, port)) = parse_tcp_hex_endpoint(raddr, rport, v6) else {
            continue;
        };
        if !ports.contains(&port) {
            continue;
        }
        *out.by_remote_ip.entry(ip).or_insert(0) += 1;
        out.raw_matches.push(line.to_string());
    }
}

#[cfg(test)]
mod tests {
    use crate::util::parse::parse_tcp_hex_endpoint;

    #[test]
    fn parse_ipv4() {
        let (ip, p) = parse_tcp_hex_endpoint("0100007F", "0801", false).expect("parse");
        assert_eq!(ip.to_string(), "127.0.0.1");
        assert_eq!(p, 2049);
    }

    #[test]
    fn parse_ipv6_loopback() {
        // ::1 as printed by /proc/net/tcp6 on little-endian x86_64
        let (ip, p) = parse_tcp_hex_endpoint("00000000000000000000000001000000", "0801", true).expect("parse");
        assert_eq!(ip.to_string(), "::1");
        assert_eq!(p, 2049);
    }

    #[test]
    fn parse_ipv6_mapped_v4() {
        // ::ffff:10.1.1.2 as printed by /proc/net/tcp6 on little-endian x86_64
        let (ip, p) = parse_tcp_hex_endpoint("0000000000000000FFFF00000201010A", "0801", true).expect("parse");
        assert_eq!(ip.to_string(), "::ffff:10.1.1.2");
        assert_eq!(p, 2049);
    }
}
