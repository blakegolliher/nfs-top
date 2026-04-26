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
use libbpf_rs::OpenObject;
use std::mem::MaybeUninit;
use skel::{NfsLatSkel, NfsLatSkelBuilder};

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
pub struct Enricher {
    _open_object: Box<MaybeUninit<OpenObject>>,
    _skel: NfsLatSkel<'static>,
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
            _open_object: open_object,
            _skel: loaded,
        })
    }
}
