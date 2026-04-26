use std::collections::HashMap;
use std::net::IpAddr;
use std::time::SystemTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitsMode {
    Auto,
    MiB,
    GiB,
    TiB,
}

impl UnitsMode {
    pub fn label(self) -> &'static str {
        match self {
            UnitsMode::Auto => "AUTO",
            UnitsMode::MiB => "MiB",
            UnitsMode::GiB => "GiB",
            UnitsMode::TiB => "TiB",
        }
    }
}

#[derive(Debug, Clone)]
pub struct OpCounters {
    pub op: String,
    pub calls: u64,
    pub bytes_sent: u64,
    pub bytes_recv: u64,
    pub rtt_ms_total: f64,
    pub exe_ms_total: f64,
}

#[derive(Debug, Clone)]
pub struct MountCounters {
    pub device: String,
    pub mountpoint: String,
    pub fstype: String,
    pub vers: Option<String>,
    pub proto: Option<String>,
    pub nconnect: Option<u32>,
    pub addr: Option<IpAddr>,
    pub clientaddr: Option<IpAddr>,
    pub options: HashMap<String, String>,
    pub ops: HashMap<String, OpCounters>,
    pub raw_block: String,
}

#[derive(Debug, Clone, Default)]
pub struct MountDerived {
    pub read_bps: f64,
    pub write_bps: f64,
    pub ops_per_sec: f64,
    pub avg_rtt_ms: Option<f64>,
    pub avg_exe_ms: Option<f64>,
    pub observed_conns: u64,
    pub observed_by_ip: Vec<(IpAddr, u64)>,
    pub per_op: Vec<OpDerived>,
}

#[derive(Debug, Clone)]
pub struct OpDerived {
    pub op: String,
    pub ops_per_sec: f64,
    pub bytes_per_sec: f64,
    pub share_pct: f64,
    pub avg_rtt_ms: Option<f64>,
    pub avg_exe_ms: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct MountView {
    pub counters: MountCounters,
    pub derived: MountDerived,
    pub resolved_ips: Vec<IpAddr>,
}

#[derive(Debug, Clone)]
pub struct ServerAgg {
    pub addr: Option<IpAddr>,
    pub hostname: String,
    pub mounts: Vec<String>,
    pub read_bps: f64,
    pub write_bps: f64,
    pub ops_per_sec: f64,
    pub avg_rtt_ms: Option<f64>,
    pub avg_exe_ms: Option<f64>,
    pub observed_conns: u64,
    pub per_op: Vec<OpDerived>,
}

#[derive(Debug, Clone, Default)]
pub struct RpcClientCounters {
    pub raw: String,
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub ts: SystemTime,
    pub dt_secs: f64,
    pub mounts: Vec<MountView>,
    pub rpc: RpcClientCounters,
    pub raw_tcp_matches: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Read,
    Write,
    Ops,
    Rtt,
    Exe,
    Mount,
    Nconnect,
    ObsConn,
}

impl SortKey {
    pub fn next(self) -> Self {
        match self {
            SortKey::Read => SortKey::Write,
            SortKey::Write => SortKey::Ops,
            SortKey::Ops => SortKey::Rtt,
            SortKey::Rtt => SortKey::Exe,
            SortKey::Exe => SortKey::Mount,
            SortKey::Mount => SortKey::Nconnect,
            SortKey::Nconnect => SortKey::ObsConn,
            SortKey::ObsConn => SortKey::Read,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            SortKey::Read => "read",
            SortKey::Write => "write",
            SortKey::Ops => "ops",
            SortKey::Rtt => "rtt",
            SortKey::Exe => "exe",
            SortKey::Mount => "mount",
            SortKey::Nconnect => "nconnect",
            SortKey::ObsConn => "obsconn",
        }
    }
}
