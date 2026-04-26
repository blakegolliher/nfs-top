//! Optional eBPF latency enricher.
//!
//! Built only when the `ebpf` cargo feature is enabled. Even when built,
//! this module is purely additive: failure to load (no CAP_BPF, no BTF,
//! verifier rejection) returns an error to the caller, who logs it once
//! and continues with the existing /proc-derived sampler unchanged.
//!
//! v0 scaffolding: loads the skeleton, attaches no useful probes yet.
//! Real probes land in the next commit.

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

use libbpf_rs::skel::{OpenSkel, SkelBuilder};
use libbpf_rs::OpenObject;
use std::mem::MaybeUninit;
use skel::{NfsLatSkel, NfsLatSkelBuilder};

/// Loaded BPF skeleton. Drop unloads the program.
///
/// The `OpenObject` storage is held in a Box so the loaded `NfsLatSkel`'s
/// internal references remain valid for the lifetime of `Enricher`.
pub struct Enricher {
    _open_object: Box<MaybeUninit<OpenObject>>,
    _skel: NfsLatSkel<'static>,
}

impl Enricher {
    /// Try to open and load the BPF program. Returns Err on any failure
    /// (kernel too old, no BTF, no CAP_BPF, verifier rejection).
    pub fn try_new() -> Result<Self> {
        let mut open_object = Box::new(MaybeUninit::uninit());
        let builder = NfsLatSkelBuilder::default();
        // Tie the open skel's lifetime to the boxed OpenObject. The Box
        // is then stored alongside the loaded skel in this struct, so
        // the storage outlives every reference into it.
        let open = {
            let storage: &mut MaybeUninit<OpenObject> = &mut *open_object;
            // Promote the &mut to 'static for the duration of this call;
            // safety relies on us keeping `open_object` boxed and pinned
            // inside `Self` for as long as `_skel` is alive.
            let storage: &'static mut MaybeUninit<OpenObject> =
                unsafe { std::mem::transmute(storage) };
            builder.open(storage).context("opening BPF skeleton")?
        };
        let loaded = open.load().context("loading BPF skeleton")?;
        Ok(Self {
            _open_object: open_object,
            _skel: loaded,
        })
    }
}
