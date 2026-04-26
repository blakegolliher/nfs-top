use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::sync::Arc;
use std::time::SystemTime;

use std::net::IpAddr;

use crate::model::derive::host_from_device;
use crate::model::types::{MountView, OpDerived, ServerAgg, Snapshot, SortKey, UnitsMode};
use crate::util::ringbuf::RingBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Overview,
    RpcMix,
    Trends,
    Connections,
    Raw,
    Servers,
    Help,
}

impl Tab {
    pub fn titles() -> [&'static str; 7] {
        ["Overview", "RPC Mix", "Trends", "Connections", "Raw", "Servers", "Help"]
    }

    pub fn next(self) -> Self {
        match self {
            Tab::Overview => Tab::RpcMix,
            Tab::RpcMix => Tab::Trends,
            Tab::Trends => Tab::Connections,
            Tab::Connections => Tab::Raw,
            Tab::Raw => Tab::Servers,
            Tab::Servers => Tab::Help,
            Tab::Help => Tab::Overview,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Tab::Overview => Tab::Help,
            Tab::RpcMix => Tab::Overview,
            Tab::Trends => Tab::RpcMix,
            Tab::Connections => Tab::Trends,
            Tab::Raw => Tab::Connections,
            Tab::Servers => Tab::Raw,
            Tab::Help => Tab::Servers,
        }
    }

    pub fn idx(self) -> usize {
        match self {
            Tab::Overview => 0,
            Tab::RpcMix => 1,
            Tab::Trends => 2,
            Tab::Connections => 3,
            Tab::Raw => 4,
            Tab::Servers => 5,
            Tab::Help => 6,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PercentileMode {
    All,
    Avg,
    P90,
    P95,
    P99,
}

impl PercentileMode {
    pub fn next(self) -> Self {
        match self {
            PercentileMode::All => PercentileMode::Avg,
            PercentileMode::Avg => PercentileMode::P90,
            PercentileMode::P90 => PercentileMode::P95,
            PercentileMode::P95 => PercentileMode::P99,
            PercentileMode::P99 => PercentileMode::All,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            PercentileMode::All => "all",
            PercentileMode::Avg => "avg",
            PercentileMode::P90 => "p90",
            PercentileMode::P95 => "p95",
            PercentileMode::P99 => "p99",
        }
    }
}

#[derive(Debug)]
pub struct MountHistory {
    pub read_bps: RingBuf<f64>,
    pub write_bps: RingBuf<f64>,
    pub read_lat_ms: RingBuf<f64>,
    pub write_lat_ms: RingBuf<f64>,
}

impl MountHistory {
    fn new(history: usize) -> Self {
        Self {
            read_bps: RingBuf::new(history),
            write_bps: RingBuf::new(history),
            read_lat_ms: RingBuf::new(history),
            write_lat_ms: RingBuf::new(history),
        }
    }
}

#[derive(Debug)]
pub struct ServerHistory {
    pub read_bps: RingBuf<f64>,
    pub write_bps: RingBuf<f64>,
    pub ops: RingBuf<f64>,
    pub rtt_ms: RingBuf<f64>,
}

impl ServerHistory {
    fn new(history: usize) -> Self {
        Self {
            read_bps: RingBuf::new(history),
            write_bps: RingBuf::new(history),
            ops: RingBuf::new(history),
            rtt_ms: RingBuf::new(history),
        }
    }
}

pub struct App {
    pub tab: Tab,
    pub selected: usize,
    pub server_selected: usize,
    pub paused: bool,
    pub filter: String,
    pub sort: SortKey,
    pub units: UnitsMode,
    pub interval: Arc<AtomicU64>,
    pub snapshot: Option<Snapshot>,
    pub read_hist: RingBuf<f64>,
    pub write_hist: RingBuf<f64>,
    pub ops_hist: RingBuf<f64>,
    pub rtt_hist: RingBuf<f64>,
    pub cumulative_read_bytes: f64,
    pub cumulative_write_bytes: f64,
    pub last_error: Option<String>,
    pub last_sample: Option<SystemTime>,
    pub percentile_mode: PercentileMode,
    history_len: usize,
    mount_histories: HashMap<String, MountHistory>,
    server_histories: HashMap<Option<IpAddr>, ServerHistory>,
}

impl App {
    pub fn new(history: usize, units: UnitsMode, interval: Arc<AtomicU64>, sort: SortKey, filter: String) -> Self {
        Self {
            tab: Tab::Overview,
            selected: 0,
            server_selected: 0,
            paused: false,
            filter,
            sort,
            units,
            interval,
            last_error: None,
            snapshot: None,
            read_hist: RingBuf::new(history),
            write_hist: RingBuf::new(history),
            ops_hist: RingBuf::new(history),
            rtt_hist: RingBuf::new(history),
            cumulative_read_bytes: 0.0,
            cumulative_write_bytes: 0.0,
            last_sample: None,
            percentile_mode: PercentileMode::All,
            history_len: history,
            mount_histories: HashMap::new(),
            server_histories: HashMap::new(),
        }
    }

    pub fn reset_baseline(&mut self) {
        self.read_hist.clear();
        self.write_hist.clear();
        self.ops_hist.clear();
        self.rtt_hist.clear();
        self.cumulative_read_bytes = 0.0;
        self.cumulative_write_bytes = 0.0;
        self.mount_histories.clear();
        self.server_histories.clear();
    }

    pub fn ingest(&mut self, snap: Snapshot) {
        if self.paused {
            return;
        }
        let filtered = snap.mounts.iter().filter(|m| self.matches_filter(m)).collect::<Vec<_>>();
        let total_read: f64 = filtered.iter().map(|m| m.derived.read_bps).sum();
        let total_write: f64 = filtered.iter().map(|m| m.derived.write_bps).sum();
        let total_ops: f64 = filtered.iter().map(|m| m.derived.ops_per_sec).sum();
        let total_rtt = filtered.iter().filter_map(|m| m.derived.avg_rtt_ms).sum::<f64>();
        let rtt_count = filtered.iter().filter(|m| m.derived.avg_rtt_ms.is_some()).count();
        self.read_hist.push(total_read);
        self.write_hist.push(total_write);
        self.ops_hist.push(total_ops);
        if snap.dt_secs > 0.0 && snap.dt_secs.is_finite() {
            self.cumulative_read_bytes += total_read * snap.dt_secs;
            self.cumulative_write_bytes += total_write * snap.dt_secs;
        }
        self.rtt_hist.push(if rtt_count > 0 { total_rtt / (rtt_count as f64) } else { 0.0 });
        for m in &snap.mounts {
            let h = self
                .mount_histories
                .entry(m.counters.mountpoint.clone())
                .or_insert_with(|| MountHistory::new(self.history_len));
            h.read_bps.push(m.derived.read_bps);
            h.write_bps.push(m.derived.write_bps);
            let read_lat = m
                .derived
                .per_op
                .iter()
                .find(|o| o.op == "READ")
                .and_then(|o| o.avg_rtt_ms)
                .unwrap_or(0.0);
            let write_lat = m
                .derived
                .per_op
                .iter()
                .find(|o| o.op == "WRITE")
                .and_then(|o| o.avg_rtt_ms)
                .unwrap_or(0.0);
            h.read_lat_ms.push(read_lat);
            h.write_lat_ms.push(write_lat);
        }
        // Aggregate per-server totals and push to server histories
        let servers = Self::aggregate_servers_from(&snap.mounts, self.sort);
        for srv in &servers {
            let h = self
                .server_histories
                .entry(srv.addr)
                .or_insert_with(|| ServerHistory::new(self.history_len));
            h.read_bps.push(srv.read_bps);
            h.write_bps.push(srv.write_bps);
            h.ops.push(srv.ops_per_sec);
            h.rtt_ms.push(srv.avg_rtt_ms.unwrap_or(0.0));
        }
        self.last_sample = Some(snap.ts);
        self.snapshot = Some(snap);
        let max = self.visible_mounts().len().saturating_sub(1);
        self.selected = self.selected.min(max);
        let server_max = servers.len().saturating_sub(1);
        self.server_selected = self.server_selected.min(server_max);
    }

    pub fn visible_mounts(&self) -> Vec<&MountView> {
        let mut v: Vec<&MountView> = self
            .snapshot
            .as_ref()
            .map(|s| s.mounts.iter().collect())
            .unwrap_or_default();

        if !self.filter.is_empty() {
            v.retain(|m| self.matches_filter(m));
        }

        v.sort_by(|a, b| self.compare_mounts(a, b));
        v
    }

    fn compare_mounts(&self, a: &MountView, b: &MountView) -> Ordering {
        match self.sort {
            SortKey::Read => b.derived.read_bps.partial_cmp(&a.derived.read_bps).unwrap_or(Ordering::Equal),
            SortKey::Write => b.derived.write_bps.partial_cmp(&a.derived.write_bps).unwrap_or(Ordering::Equal),
            SortKey::Ops => b.derived.ops_per_sec.partial_cmp(&a.derived.ops_per_sec).unwrap_or(Ordering::Equal),
            SortKey::Rtt => b.derived.avg_rtt_ms.partial_cmp(&a.derived.avg_rtt_ms).unwrap_or(Ordering::Equal),
            SortKey::Exe => b.derived.avg_exe_ms.partial_cmp(&a.derived.avg_exe_ms).unwrap_or(Ordering::Equal),
            SortKey::Mount => a.counters.mountpoint.cmp(&b.counters.mountpoint),
            SortKey::Nconnect => b.counters.nconnect.cmp(&a.counters.nconnect),
            SortKey::ObsConn => b.derived.observed_conns.cmp(&a.derived.observed_conns),
        }
    }

    pub fn selected_mount(&self) -> Option<&MountView> {
        self.visible_mounts().get(self.selected).copied()
    }

    pub fn selected_mount_history(&self) -> Option<&MountHistory> {
        let mount = self.selected_mount()?;
        self.mount_histories.get(&mount.counters.mountpoint)
    }

    fn aggregate_servers_from(mounts: &[MountView], sort: SortKey) -> Vec<ServerAgg> {
        let mut by_addr: HashMap<Option<IpAddr>, Vec<&MountView>> = HashMap::new();
        for m in mounts {
            by_addr.entry(m.counters.addr).or_default().push(m);
        }
        let mut servers: Vec<ServerAgg> = by_addr
            .into_iter()
            .map(|(addr, group)| {
                let read_bps: f64 = group.iter().map(|m| m.derived.read_bps).sum();
                let write_bps: f64 = group.iter().map(|m| m.derived.write_bps).sum();
                let ops_per_sec: f64 = group.iter().map(|m| m.derived.ops_per_sec).sum();
                let observed_conns: u64 = group.iter().map(|m| m.derived.observed_conns).sum();

                // Ops-weighted average latency
                let avg_rtt_ms = {
                    let (weighted_sum, total_weight) = group.iter().fold((0.0, 0.0), |(ws, tw), m| {
                        if let Some(rtt) = m.derived.avg_rtt_ms {
                            (ws + rtt * m.derived.ops_per_sec, tw + m.derived.ops_per_sec)
                        } else {
                            (ws, tw)
                        }
                    });
                    if total_weight > 0.0 { Some(weighted_sum / total_weight) } else { None }
                };
                let avg_exe_ms = {
                    let (weighted_sum, total_weight) = group.iter().fold((0.0, 0.0), |(ws, tw), m| {
                        if let Some(exe) = m.derived.avg_exe_ms {
                            (ws + exe * m.derived.ops_per_sec, tw + m.derived.ops_per_sec)
                        } else {
                            (ws, tw)
                        }
                    });
                    if total_weight > 0.0 { Some(weighted_sum / total_weight) } else { None }
                };

                // Merge per-op stats across mounts
                let mut op_map: HashMap<String, (f64, f64, f64, f64, f64, f64)> = HashMap::new();
                // (ops_sum, bytes_sum, rtt_weighted_sum, rtt_weight, exe_weighted_sum, exe_weight)
                for m in &group {
                    for op in &m.derived.per_op {
                        let e = op_map.entry(op.op.clone()).or_default();
                        e.0 += op.ops_per_sec;
                        e.1 += op.bytes_per_sec;
                        if let Some(rtt) = op.avg_rtt_ms {
                            e.2 += rtt * op.ops_per_sec;
                            e.3 += op.ops_per_sec;
                        }
                        if let Some(exe) = op.avg_exe_ms {
                            e.4 += exe * op.ops_per_sec;
                            e.5 += op.ops_per_sec;
                        }
                    }
                }
                let total_ops_for_share: f64 = op_map.values().map(|v| v.0).sum();
                let mut per_op: Vec<OpDerived> = op_map
                    .into_iter()
                    .map(|(op, (ops_sum, bytes_sum, rtt_ws, rtt_w, exe_ws, exe_w))| OpDerived {
                        op,
                        ops_per_sec: ops_sum,
                        bytes_per_sec: bytes_sum,
                        share_pct: if total_ops_for_share > 0.0 { ops_sum / total_ops_for_share * 100.0 } else { 0.0 },
                        avg_rtt_ms: if rtt_w > 0.0 { Some(rtt_ws / rtt_w) } else { None },
                        avg_exe_ms: if exe_w > 0.0 { Some(exe_ws / exe_w) } else { None },
                    })
                    .collect();
                per_op.sort_by(|a, b| b.ops_per_sec.partial_cmp(&a.ops_per_sec).unwrap_or(Ordering::Equal));

                let hostname = group
                    .first()
                    .and_then(|m| host_from_device(&m.counters.device))
                    .unwrap_or("unknown")
                    .to_string();

                let mounts: Vec<String> = group.iter().map(|m| m.counters.mountpoint.clone()).collect();

                ServerAgg {
                    addr,
                    hostname,
                    mounts,
                    read_bps,
                    write_bps,
                    ops_per_sec,
                    avg_rtt_ms,
                    avg_exe_ms,
                    observed_conns,
                    per_op,
                }
            })
            .collect();

        servers.sort_by(|a, b| match sort {
            SortKey::Read => b.read_bps.partial_cmp(&a.read_bps).unwrap_or(Ordering::Equal),
            SortKey::Write => b.write_bps.partial_cmp(&a.write_bps).unwrap_or(Ordering::Equal),
            SortKey::Ops => b.ops_per_sec.partial_cmp(&a.ops_per_sec).unwrap_or(Ordering::Equal),
            SortKey::Rtt => b.avg_rtt_ms.partial_cmp(&a.avg_rtt_ms).unwrap_or(Ordering::Equal),
            SortKey::Exe => b.avg_exe_ms.partial_cmp(&a.avg_exe_ms).unwrap_or(Ordering::Equal),
            SortKey::ObsConn => b.observed_conns.cmp(&a.observed_conns),
            _ => b.ops_per_sec.partial_cmp(&a.ops_per_sec).unwrap_or(Ordering::Equal),
        });

        servers
    }

    pub fn aggregate_servers(&self) -> Vec<ServerAgg> {
        match self.snapshot.as_ref() {
            Some(snap) => Self::aggregate_servers_from(&snap.mounts, self.sort),
            None => Vec::new(),
        }
    }

    pub fn selected_server(&self) -> Option<ServerAgg> {
        self.aggregate_servers().into_iter().nth(self.server_selected)
    }

    pub fn selected_server_history(&self) -> Option<&ServerHistory> {
        let srv = self.selected_server()?;
        self.server_histories.get(&srv.addr)
    }

    pub fn interval_ms(&self) -> u64 {
        self.interval.load(AtomicOrdering::Relaxed)
    }

    pub fn increase_interval(&self) {
        let cur = self.interval.load(AtomicOrdering::Relaxed);
        self.interval.store((cur + 100).min(10_000), AtomicOrdering::Relaxed);
    }

    pub fn decrease_interval(&self) {
        let cur = self.interval.load(AtomicOrdering::Relaxed);
        self.interval.store(cur.saturating_sub(100).max(100), AtomicOrdering::Relaxed);
    }

    fn matches_filter(&self, m: &MountView) -> bool {
        if self.filter.is_empty() {
            return true;
        }
        let q = self.filter.to_lowercase();
        m.counters.mountpoint.to_lowercase().contains(&q) || m.counters.device.to_lowercase().contains(&q)
    }
}
