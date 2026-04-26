use anyhow::Result;
use std::collections::HashMap;
use std::fs;

use crate::model::types::RpcClientCounters;

pub fn read_rpc_client() -> Result<RpcClientCounters> {
    let raw = fs::read_to_string("/proc/net/rpc/nfs")?;
    let mut fields = HashMap::new();
    for line in raw.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 2 {
            continue;
        }
        for (i, val) in cols.iter().skip(1).enumerate() {
            if let Ok(v) = (*val).parse::<u64>() {
                fields.insert(format!("{}.{}", cols[0], i), v);
            }
        }
    }
    Ok(RpcClientCounters { fields, raw })
}
