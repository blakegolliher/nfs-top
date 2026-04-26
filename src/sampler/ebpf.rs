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
pub const OP_OTHER: u16 = 0;
pub const OP_READ: u16 = 1;
pub const OP_WRITE: u16 = 2;
pub const OP_COMMIT: u16 = 3;

pub fn op_name(id: u16) -> &'static str {
    match id {
        OP_READ => "READ",
        OP_WRITE => "WRITE",
        OP_COMMIT => "COMMIT",
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
    /// Last-seen absolute count for every (op_id, bucket) we've ever
    /// observed in the kernel `hist` map. Used to compute per-tick deltas
    /// without resetting the map (snapshot-and-diff).
    prev: HashMap<(u16, u16), u64>,
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
    /// fold the per-tick deltas into a `BpfLatency`. Returns `None` if no
    /// op saw any new samples this tick.
    pub fn snapshot(&mut self) -> Result<Option<BpfLatency>> {
        let map = &self.skel.maps.hist;
        let mut per_op: HashMap<u16, [u64; BUCKETS]> = HashMap::new();
        let mut total_samples: u64 = 0;

        for key_bytes in MapCore::keys(map) {
            let Some(val) = MapCore::lookup(map, &key_bytes, MapFlags::ANY)? else {
                continue;
            };
            if key_bytes.len() < 4 || val.len() < 8 {
                continue;
            }
            let op_id = u16::from_ne_bytes([key_bytes[0], key_bytes[1]]);
            let bucket = u16::from_ne_bytes([key_bytes[2], key_bytes[3]]);
            let curr = u64::from_ne_bytes(val[..8].try_into().unwrap());
            let entry = self.prev.entry((op_id, bucket)).or_insert(0);
            let delta = curr.saturating_sub(*entry);
            *entry = curr;
            if delta == 0 {
                continue;
            }
            let bucket = (bucket as usize).min(BUCKETS - 1);
            per_op.entry(op_id).or_insert([0u64; BUCKETS])[bucket] += delta;
            total_samples = total_samples.saturating_add(delta);
        }

        if per_op.is_empty() {
            return Ok(None);
        }

        let mut out: Vec<BpfOpLatency> = per_op
            .into_iter()
            .map(|(op_id, buckets)| BpfOpLatency {
                op: op_name(op_id).to_string(),
                dist: dist_from_buckets(&buckets),
            })
            .collect();
        out.sort_by(|a, b| b.dist.samples.cmp(&a.dist.samples));

        Ok(Some(BpfLatency {
            per_op: out,
            total_samples,
        }))
    }
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
