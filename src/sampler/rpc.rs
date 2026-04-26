use anyhow::{Context, Result};
use std::fs;

use crate::model::types::RpcClientCounters;

const RPC_NFS_PATH: &str = "/proc/net/rpc/nfs";

pub fn read_rpc_client() -> Result<RpcClientCounters> {
    let raw = fs::read_to_string(RPC_NFS_PATH)
        .with_context(|| format!("reading {RPC_NFS_PATH}"))?;
    Ok(RpcClientCounters { raw })
}
