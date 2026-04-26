use std::collections::HashMap;
use std::net::{IpAddr, ToSocketAddrs};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct DnsCache {
    ttl: Duration,
    map: HashMap<String, (Instant, Vec<IpAddr>)>,
}

impl DnsCache {
    pub fn new(ttl: Duration) -> Self {
        Self { ttl, map: HashMap::new() }
    }

    pub fn resolve(&mut self, host: &str) -> Vec<IpAddr> {
        if let Some((ts, ips)) = self.map.get(host)
            && ts.elapsed() < self.ttl
        {
            return ips.clone();
        }

        let mut ips = Vec::new();
        if let Ok(addrs) = (host, 2049).to_socket_addrs() {
            for a in addrs {
                if !ips.contains(&a.ip()) {
                    ips.push(a.ip());
                }
            }
        }

        self.map.insert(host.to_string(), (Instant::now(), ips.clone()));
        ips
    }
}
