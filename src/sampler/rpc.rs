use anyhow::Result;
use std::fs;

use crate::model::types::RpcClientCounters;

pub fn read_rpc_client() -> Result<RpcClientCounters> {
    let raw = fs::read_to_string("/proc/net/rpc/nfs")?;
    Ok(RpcClientCounters { raw })
}
