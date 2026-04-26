use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;

pub fn read_mount_options() -> Result<HashMap<String, HashMap<String, String>>> {
    let data = fs::read_to_string("/proc/mounts")
        .or_else(|_| fs::read_to_string("/etc/mtab"))
        .context("reading /proc/mounts (and /etc/mtab fallback)")?;
    let mut out = HashMap::new();
    for line in data.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 4 {
            continue;
        }
        let mountpoint = cols[1].to_string();
        let mut opts = HashMap::new();
        for tok in cols[3].split(',') {
            if let Some((k, v)) = tok.split_once('=') {
                opts.insert(k.to_string(), v.to_string());
            } else {
                opts.insert(tok.to_string(), String::new());
            }
        }
        out.insert(mountpoint, opts);
    }
    Ok(out)
}
