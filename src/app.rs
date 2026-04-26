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
    Hist,
    Connections,
    Raw,
    Servers,
    Help,
}

impl Tab {
    pub fn titles() -> [&'static str; 8] {
        ["Overview", "RPC Mix", "Trends", "Hist", "Connections", "Raw", "Servers", "Help"]
    }

    pub fn next(self) -> Self {
        match self {
            Tab::Overview => Tab::RpcMix,
            Tab::RpcMix => Tab::Trends,
            Tab::Trends => Tab::Hist,
            Tab::Hist => Tab::Connections,
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
            Tab::Hist => Tab::Trends,
            Tab::Connections => Tab::Hist,
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
            Tab::Hist => 3,
            Tab::Connections => 4,
            Tab::Raw => 5,
            Tab::Servers => 6,
            Tab::Help => 7,
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
    filter_lower: String,
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
    /// Indices into snapshot.mounts in display (sorted+filtered) order.
    /// Recomputed only on ingest or sort change.
    cached_visible_idx: Vec<usize>,
    /// Per-server aggregates in display (sorted) order. Recomputed only on
    /// ingest or sort change. UI reads via `aggregate_servers()`.
    cached_servers: Vec<ServerAgg>,
}

impl App {
    pub fn new(history: usize, units: UnitsMode, interval: Arc<AtomicU64>, sort: SortKey, filter: String) -> Self {
        let filter_lower = filter.to_lowercase();
        Self {
            tab: Tab::Overview,
            selected: 0,
            server_selected: 0,
            paused: false,
            filter,
            filter_lower,
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
            cached_visible_idx: Vec::new(),
            cached_servers: Vec::new(),
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
        self.update_global_history(&snap);
        self.update_mount_history(&snap.mounts);
        self.cached_servers = Self::aggregate_servers_from(&snap.mounts, self.sort);
        self.update_server_history();
        self.cached_visible_idx = self.compute_visible_indices(&snap.mounts);
        self.last_sample = Some(snap.ts);
        self.snapshot = Some(snap);
        self.clamp_selection();
    }

    fn update_global_history(&mut self, snap: &Snapshot) {
        let mut total_read = 0.0;
        let mut total_write = 0.0;
        let mut total_ops = 0.0;
        let mut total_rtt = 0.0;
        let mut rtt_count = 0usize;
        for m in snap.mounts.iter().filter(|m| self.matches_filter(m)) {
            total_read += m.derived.read_bps;
            total_write += m.derived.write_bps;
            total_ops += m.derived.ops_per_sec;
            if let Some(rtt) = m.derived.avg_rtt_ms {
                total_rtt += rtt;
                rtt_count += 1;
            }
        }
        self.read_hist.push(total_read);
        self.write_hist.push(total_write);
        self.ops_hist.push(total_ops);
        self.rtt_hist.push(if rtt_count > 0 { total_rtt / rtt_count as f64 } else { 0.0 });
        if snap.dt_secs > 0.0 && snap.dt_secs.is_finite() {
            self.cumulative_read_bytes += total_read * snap.dt_secs;
            self.cumulative_write_bytes += total_write * snap.dt_secs;
        }
    }

    fn update_mount_history(&mut self, mounts: &[MountView]) {
        for m in mounts {
            let h = self
                .mount_histories
                .entry(m.counters.mountpoint.clone())
                .or_insert_with(|| MountHistory::new(self.history_len));
            h.read_bps.push(m.derived.read_bps);
            h.write_bps.push(m.derived.write_bps);
            h.read_lat_ms.push(op_lat(&m.derived.per_op, "READ"));
            h.write_lat_ms.push(op_lat(&m.derived.per_op, "WRITE"));
        }
    }

    fn update_server_history(&mut self) {
        for srv in &self.cached_servers {
            let h = self
                .server_histories
                .entry(srv.addr)
                .or_insert_with(|| ServerHistory::new(self.history_len));
            h.read_bps.push(srv.read_bps);
            h.write_bps.push(srv.write_bps);
            h.ops.push(srv.ops_per_sec);
            h.rtt_ms.push(srv.avg_rtt_ms.unwrap_or(0.0));
        }
    }

    fn compute_visible_indices(&self, mounts: &[MountView]) -> Vec<usize> {
        let mut idx: Vec<usize> = mounts
            .iter()
            .enumerate()
            .filter(|(_, m)| self.matches_filter(m))
            .map(|(i, _)| i)
            .collect();
        idx.sort_by(|&a, &b| self.compare_mounts(&mounts[a], &mounts[b]));
        idx
    }

    fn clamp_selection(&mut self) {
        self.selected = self.selected.min(self.cached_visible_idx.len().saturating_sub(1));
        self.server_selected = self.server_selected.min(self.cached_servers.len().saturating_sub(1));
    }

    /// Cycle the sort key. Re-sorts the cached visible mounts and server
    /// aggregates so the next render reflects the new order without waiting
    /// for the next sample.
    pub fn cycle_sort(&mut self) {
        self.sort = self.sort.next();
        let sort = self.sort;
        if let Some(snap) = self.snapshot.as_ref() {
            let mounts = &snap.mounts;
            self.cached_visible_idx
                .sort_by(|&a, &b| compare_mounts_by(&mounts[a], &mounts[b], sort));
        }
        sort_servers(&mut self.cached_servers, sort);
    }

    pub fn visible_mounts(&self) -> Vec<&MountView> {
        let Some(snap) = self.snapshot.as_ref() else { return Vec::new() };
        self.cached_visible_idx
            .iter()
            .filter_map(|&i| snap.mounts.get(i))
            .collect()
    }

    fn compare_mounts(&self, a: &MountView, b: &MountView) -> Ordering {
        compare_mounts_by(a, b, self.sort)
    }

    pub fn selected_mount(&self) -> Option<&MountView> {
        let snap = self.snapshot.as_ref()?;
        let &idx = self.cached_visible_idx.get(self.selected)?;
        snap.mounts.get(idx)
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
            .map(|(addr, group)| build_server_agg(addr, &group))
            .collect();
        sort_servers(&mut servers, sort);
        servers
    }

    pub fn aggregate_servers(&self) -> &[ServerAgg] {
        &self.cached_servers
    }

    pub fn selected_server(&self) -> Option<&ServerAgg> {
        self.cached_servers.get(self.server_selected)
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
        if self.filter_lower.is_empty() {
            return true;
        }
        let q = &self.filter_lower;
        m.counters.mountpoint.to_lowercase().contains(q) || m.counters.device.to_lowercase().contains(q)
    }
}

fn compare_mounts_by(a: &MountView, b: &MountView, sort: SortKey) -> Ordering {
    match sort {
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

fn op_lat(per_op: &[OpDerived], op_name: &str) -> f64 {
    per_op
        .iter()
        .find(|o| o.op == op_name)
        .and_then(|o| o.avg_rtt_ms)
        .unwrap_or(0.0)
}

/// Ops-weighted mean: each (value, weight) contributes value*weight to the sum
/// when value is Some. Returns None if the total weight is zero.
fn ops_weighted_mean<I>(items: I) -> Option<f64>
where
    I: IntoIterator<Item = (Option<f64>, f64)>,
{
    let (sum, weight) = items.into_iter().fold((0.0_f64, 0.0_f64), |(s, w), (v, wt)| match v {
        Some(x) => (s + x * wt, w + wt),
        None => (s, w),
    });
    (weight > 0.0).then_some(sum / weight)
}

#[derive(Default)]
struct OpAccum {
    ops_sum: f64,
    bytes_sum: f64,
    rtt_weighted_sum: f64,
    rtt_weight: f64,
    exe_weighted_sum: f64,
    exe_weight: f64,
}

fn build_server_agg(addr: Option<IpAddr>, group: &[&MountView]) -> ServerAgg {
    let read_bps: f64 = group.iter().map(|m| m.derived.read_bps).sum();
    let write_bps: f64 = group.iter().map(|m| m.derived.write_bps).sum();
    let ops_per_sec: f64 = group.iter().map(|m| m.derived.ops_per_sec).sum();
    let observed_conns: u64 = group.iter().map(|m| m.derived.observed_conns).sum();

    let avg_rtt_ms = ops_weighted_mean(group.iter().map(|m| (m.derived.avg_rtt_ms, m.derived.ops_per_sec)));
    let avg_exe_ms = ops_weighted_mean(group.iter().map(|m| (m.derived.avg_exe_ms, m.derived.ops_per_sec)));

    let mut op_map: HashMap<String, OpAccum> = HashMap::new();
    for m in group {
        for op in &m.derived.per_op {
            let e = op_map.entry(op.op.clone()).or_default();
            e.ops_sum += op.ops_per_sec;
            e.bytes_sum += op.bytes_per_sec;
            if let Some(rtt) = op.avg_rtt_ms {
                e.rtt_weighted_sum += rtt * op.ops_per_sec;
                e.rtt_weight += op.ops_per_sec;
            }
            if let Some(exe) = op.avg_exe_ms {
                e.exe_weighted_sum += exe * op.ops_per_sec;
                e.exe_weight += op.ops_per_sec;
            }
        }
    }
    let total_ops_for_share: f64 = op_map.values().map(|v| v.ops_sum).sum();
    let mut per_op: Vec<OpDerived> = op_map
        .into_iter()
        .map(|(op, a)| OpDerived {
            op,
            ops_per_sec: a.ops_sum,
            bytes_per_sec: a.bytes_sum,
            share_pct: if total_ops_for_share > 0.0 { a.ops_sum / total_ops_for_share * 100.0 } else { 0.0 },
            avg_rtt_ms: (a.rtt_weight > 0.0).then_some(a.rtt_weighted_sum / a.rtt_weight),
            avg_exe_ms: (a.exe_weight > 0.0).then_some(a.exe_weighted_sum / a.exe_weight),
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
}

fn sort_servers(servers: &mut [ServerAgg], sort: SortKey) {
    servers.sort_by(|a, b| match sort {
        SortKey::Read => b.read_bps.partial_cmp(&a.read_bps).unwrap_or(Ordering::Equal),
        SortKey::Write => b.write_bps.partial_cmp(&a.write_bps).unwrap_or(Ordering::Equal),
        SortKey::Ops => b.ops_per_sec.partial_cmp(&a.ops_per_sec).unwrap_or(Ordering::Equal),
        SortKey::Rtt => b.avg_rtt_ms.partial_cmp(&a.avg_rtt_ms).unwrap_or(Ordering::Equal),
        SortKey::Exe => b.avg_exe_ms.partial_cmp(&a.avg_exe_ms).unwrap_or(Ordering::Equal),
        SortKey::ObsConn => b.observed_conns.cmp(&a.observed_conns),
        // No meaningful per-server ordering for Mount/Nconnect; fall back to Ops.
        SortKey::Mount | SortKey::Nconnect => {
            b.ops_per_sec.partial_cmp(&a.ops_per_sec).unwrap_or(Ordering::Equal)
        }
    });
}
