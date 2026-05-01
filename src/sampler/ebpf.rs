//! Optional eBPF latency enricher.
//!
//! Built only when the `ebpf` cargo feature is enabled. Even when built,
//! this module is purely additive: failure to load (no CAP_BPF, no BTF,
//! verifier rejection) returns an error to the caller, who logs it once
//! and continues with the existing /proc-derived sampler unchanged.

#![cfg(feature = "ebpf")]

use anyhow::{Context, Result};

mod skel {
    #![allow(clippy::all)]
    #![allow(dead_code)]
    #![allow(non_snake_case)]
    #![allow(non_camel_case_types)]
    #![allow(unused)]
    include!(concat!(env!("OUT_DIR"), "/nfs_lat.skel.rs"));
}

use libbpf_rs::skel::{OpenSkel, Skel, SkelBuilder};
use libbpf_rs::{MapCore, MapFlags, OpenObject};
use std::collections::HashMap;
use std::mem::MaybeUninit;
use skel::{NfsLatSkel, NfsLatSkelBuilder};

use crate::model::types::{BpfLatency, BpfOpLatency, LatencyDist};
use crate::sampler::hist::{self, BUCKETS};

/// Op identifiers, in lockstep with `enum nfs_op_id` in src/bpf/nfs_lat.bpf.h.
/// Names match what mountstats prints so users can cross-reference rows
/// in the Hist tab against /proc/self/mountstats. ID 0 is intentionally
/// unused so a zero-initialized key is unambiguously invalid; `op_name`
/// still maps it to "OTHER" via the catchall arm for defensive logging.
pub const OP_READ: u16 = 1;
pub const OP_WRITE: u16 = 2;
pub const OP_COMMIT: u16 = 3;
pub const OP_GETATTR: u16 = 4;
pub const OP_SETATTR: u16 = 5;
pub const OP_LOOKUP: u16 = 6;
pub const OP_ACCESS: u16 = 7;
pub const OP_CREATE: u16 = 8;
pub const OP_REMOVE: u16 = 9;
pub const OP_RENAME: u16 = 10;
pub const OP_LINK: u16 = 11;
pub const OP_SYMLINK: u16 = 12;
pub const OP_MKDIR: u16 = 13;
pub const OP_RMDIR: u16 = 14;
pub const OP_MKNOD: u16 = 15;
pub const OP_FSYNC: u16 = 16;
pub const OP_OPEN: u16 = 17;

pub fn op_name(id: u16) -> &'static str {
    match id {
        OP_READ => "READ",
        OP_WRITE => "WRITE",
        OP_COMMIT => "COMMIT",
        OP_GETATTR => "GETATTR",
        OP_SETATTR => "SETATTR",
        OP_LOOKUP => "LOOKUP",
        OP_ACCESS => "ACCESS",
        OP_CREATE => "CREATE",
        OP_REMOVE => "REMOVE",
        OP_RENAME => "RENAME",
        OP_LINK => "LINK",
        OP_SYMLINK => "SYMLINK",
        OP_MKDIR => "MKDIR",
        OP_RMDIR => "RMDIR",
        OP_MKNOD => "MKNOD",
        OP_FSYNC => "FSYNC",
        OP_OPEN => "OPEN",
        _ => "OTHER",
    }
}

/// Loaded BPF skeleton with attached probes. Drop detaches and unloads.
///
/// The `OpenObject` storage is held in a Box so the loaded `NfsLatSkel`'s
/// internal references remain valid for the lifetime of `Enricher`.
///
/// **Field order is load-bearing.** Rust drops fields in declaration
/// order, and `skel` holds raw pointers into the `OpenObject` storage
/// (we faked the `'static` bound via transmute). `skel` must therefore
/// be declared before `_open_object` so it's dropped first; reversing
/// these two fields will segfault on Drop.
pub struct Enricher {
    skel: NfsLatSkel<'static>,
    _open_object: Box<MaybeUninit<OpenObject>>,
    /// Last-seen absolute count for every (dev, op_id, bucket) we've ever
    /// observed in the kernel `hist` map. Used to compute per-tick deltas
    /// without resetting the map (snapshot-and-diff).
    prev: HashMap<(u32, u16, u16), u64>,
}

impl Enricher {
    /// Open, load, and attach the BPF programs. Returns Err on any failure
    /// (kernel too old, no BTF, no CAP_BPF, verifier rejection, missing
    /// tracepoint). Caller logs once and continues with /proc only.
    pub fn try_new() -> Result<Self> {
        let mut open_object = Box::new(MaybeUninit::uninit());
        let builder = NfsLatSkelBuilder::default();
        let open = {
            let storage: &mut MaybeUninit<OpenObject> = &mut *open_object;
            // Promote &mut to 'static; safety relies on the Box being held
            // alongside the loaded skel in Self for the skel's lifetime.
            let storage: &'static mut MaybeUninit<OpenObject> =
                unsafe { std::mem::transmute(storage) };
            builder.open(storage).context("opening BPF skeleton")?
        };
        let mut loaded = open.load().context("loading BPF skeleton")?;
        loaded.attach().context("attaching BPF tracepoints")?;
        Ok(Self {
            skel: loaded,
            _open_object: open_object,
            prev: HashMap::new(),
        })
    }

    /// Walk the kernel `hist` map, diff against the previous snapshot, and
    /// fold the per-tick deltas into one `BpfLatency` per super_block dev.
    /// Devices with no new samples this tick are absent from the map; a
    /// dev_id of 0 collects samples whose init probe couldn't resolve the
    /// inode chain.
    pub fn snapshot(&mut self) -> Result<HashMap<u32, BpfLatency>> {
        let map = &self.skel.maps.hist;
        let mut items: Vec<(u32, u16, u16, u64)> = Vec::new();
        for key_bytes in MapCore::keys(map) {
            let Some(val) = MapCore::lookup(map, &key_bytes, MapFlags::ANY)? else {
                continue;
            };
            if key_bytes.len() < 8 || val.len() < 8 {
                continue;
            }
            // Layout: [dev:4][op_id:2][bucket:2] (matches `struct hist_key`).
            let dev = u32::from_ne_bytes(key_bytes[0..4].try_into().unwrap());
            let op_id = u16::from_ne_bytes([key_bytes[4], key_bytes[5]]);
            let bucket = u16::from_ne_bytes([key_bytes[6], key_bytes[7]]);
            let curr = u64::from_ne_bytes(val[..8].try_into().unwrap());
            items.push((dev, op_id, bucket, curr));
        }
        Ok(fold_deltas(&mut self.prev, items))
    }
}

/// Apply per-(dev, op, bucket) deltas against `prev` and produce one
/// `BpfLatency` per dev that saw new samples this tick. Pure function so
/// the per-dev folding logic is unit-testable without a live BPF map.
fn fold_deltas(
    prev: &mut HashMap<(u32, u16, u16), u64>,
    items: impl IntoIterator<Item = (u32, u16, u16, u64)>,
) -> HashMap<u32, BpfLatency> {
    let mut per_dev: HashMap<u32, HashMap<u16, [u64; BUCKETS]>> = HashMap::new();
    let mut totals: HashMap<u32, u64> = HashMap::new();

    for (dev, op_id, bucket, curr) in items {
        let entry = prev.entry((dev, op_id, bucket)).or_insert(0);
        let delta = curr.saturating_sub(*entry);
        *entry = curr;
        if delta == 0 {
            continue;
        }
        let bucket_idx = (bucket as usize).min(BUCKETS - 1);
        per_dev
            .entry(dev)
            .or_default()
            .entry(op_id)
            .or_insert_with(|| [0u64; BUCKETS])[bucket_idx] += delta;
        let t = totals.entry(dev).or_insert(0);
        *t = t.saturating_add(delta);
    }

    per_dev
        .into_iter()
        .map(|(dev, ops)| {
            let total = totals.get(&dev).copied().unwrap_or(0);
            (dev, build_bpf_latency(ops, total))
        })
        .collect()
}

fn build_bpf_latency(ops: HashMap<u16, [u64; BUCKETS]>, total_samples: u64) -> BpfLatency {
    let mut per_op: Vec<BpfOpLatency> = ops
        .into_iter()
        .map(|(op_id, buckets)| BpfOpLatency {
            op: op_name(op_id).to_string(),
            dist: dist_from_buckets(&buckets),
            buckets: buckets.to_vec(),
        })
        .collect();
    per_op.sort_by(|a, b| b.dist.samples.cmp(&a.dist.samples));
    BpfLatency { per_op, total_samples }
}

fn dist_from_buckets(buckets: &[u64; BUCKETS]) -> LatencyDist {
    let samples = hist::total(buckets);
    LatencyDist {
        samples,
        p50_ns: hist::percentile_ns(buckets, samples, 0.50),
        p90_ns: hist::percentile_ns(buckets, samples, 0.90),
        p99_ns: hist::percentile_ns(buckets, samples, 0.99),
        p999_ns: hist::percentile_ns(buckets, samples, 0.999),
        p9999_ns: hist::percentile_ns(buckets, samples, 0.9999),
        p99999_ns: hist::percentile_ns(buckets, samples, 0.99999),
        max_ns: hist::max_ns(buckets),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fold_deltas_splits_by_dev_and_op() {
        let mut prev: HashMap<(u32, u16, u16), u64> = HashMap::new();
        // Two devs (51, 82). Two ops on dev 51, one on dev 82.
        let items = vec![
            (51u32, OP_READ, 10u16, 5u64),
            (51, OP_READ, 12, 1),
            (51, OP_WRITE, 14, 2),
            (82, OP_READ, 10, 3),
        ];
        let out = fold_deltas(&mut prev, items);
        assert_eq!(out.len(), 2);
        let dev51 = out.get(&51).expect("dev 51");
        let dev82 = out.get(&82).expect("dev 82");
        assert_eq!(dev51.total_samples, 5 + 1 + 2);
        assert_eq!(dev82.total_samples, 3);
        // Per-op rows are sorted by sample count desc.
        assert_eq!(dev51.per_op[0].op, "READ");
        assert_eq!(dev51.per_op[0].dist.samples, 6);
        assert_eq!(dev51.per_op[1].op, "WRITE");
        assert_eq!(dev51.per_op[1].dist.samples, 2);
    }

    #[test]
    fn fold_deltas_subtracts_against_prev() {
        let mut prev: HashMap<(u32, u16, u16), u64> = HashMap::new();
        // First tick: 10 samples seen at (dev=51, READ, b=10).
        let _ = fold_deltas(&mut prev, vec![(51, OP_READ, 10, 10)]);
        // Second tick: kernel counter advanced to 13 → expect delta=3.
        let out = fold_deltas(&mut prev, vec![(51, OP_READ, 10, 13)]);
        assert_eq!(out.get(&51).unwrap().total_samples, 3);
        // Third tick: no advance → no entry produced.
        let out = fold_deltas(&mut prev, vec![(51, OP_READ, 10, 13)]);
        assert!(out.is_empty());
    }

    #[test]
    fn fold_deltas_handles_non_rw_ops() {
        // Non-RW ops must round-trip through fold_deltas with their
        // mountstats-compatible labels, sharing a dev with READ/WRITE.
        let mut prev: HashMap<(u32, u16, u16), u64> = HashMap::new();
        let items = vec![
            (51u32, OP_GETATTR, 8u16, 100u64),
            (51, OP_LOOKUP, 9, 50),
            (51, OP_ACCESS, 8, 30),
            (51, OP_READ, 14, 5),
        ];
        let out = fold_deltas(&mut prev, items);
        let dev = out.get(&51).expect("dev 51");
        assert_eq!(dev.total_samples, 100 + 50 + 30 + 5);
        // Sorted by sample count desc.
        let labels: Vec<&str> = dev.per_op.iter().map(|o| o.op.as_str()).collect();
        assert_eq!(labels, ["GETATTR", "LOOKUP", "ACCESS", "READ"]);
    }

    #[test]
    fn fold_deltas_zero_dev_is_collected_separately() {
        // dev=0 (unattributed) must not get pooled into a real mount's bucket.
        let mut prev: HashMap<(u32, u16, u16), u64> = HashMap::new();
        let out = fold_deltas(
            &mut prev,
            vec![(0, OP_READ, 10, 1), (51, OP_READ, 10, 2)],
        );
        assert_eq!(out.len(), 2);
        assert_eq!(out.get(&0).unwrap().total_samples, 1);
        assert_eq!(out.get(&51).unwrap().total_samples, 2);
    }
}
