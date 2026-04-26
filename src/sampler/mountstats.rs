use anyhow::Result;
use std::collections::HashMap;
use std::fs;

use crate::model::types::{MountCounters, OpCounters};
use crate::util::parse::{parse_ip_maybe, parse_kv_options};

pub fn read_mountstats() -> Result<Vec<MountCounters>> {
    parse_mountstats(&fs::read_to_string("/proc/self/mountstats")?)
}

pub fn parse_mountstats(input: &str) -> Result<Vec<MountCounters>> {
    let mut out = Vec::new();
    let mut block = Vec::new();

    for line in input.lines() {
        if line.starts_with("device ") && !block.is_empty() {
            if let Some(m) = parse_block(&block.join("\n")) {
                out.push(m);
            }
            block.clear();
        }
        block.push(line.to_string());
    }
    if !block.is_empty() && let Some(m) = parse_block(&block.join("\n")) {
        out.push(m);
    }

    Ok(out)
}

fn parse_block(raw: &str) -> Option<MountCounters> {
    let mut device = String::new();
    let mut mountpoint = String::new();
    let mut fstype = String::new();
    let mut options = HashMap::new();
    let mut ops = HashMap::new();

    for line in raw.lines() {
        if line.starts_with("device ") {
            let cols: Vec<&str> = line.split_whitespace().collect();
            if cols.len() >= 8 {
                device = cols[1].to_string();
                mountpoint = cols[4].to_string();
                fstype = cols[7].to_string();
            }
        }

        if line.contains("opts:") {
            let rhs = line.split_once("opts:").map(|x| x.1).unwrap_or("");
            options.extend(parse_kv_options(rhs));
        }

        if line.contains("age:") || line.contains("caps:") {
            options.extend(parse_kv_options(line));
        }

        let t = line.trim();
        if let Some((op, rest)) = t.split_once(':') {
            let key = op.trim().to_uppercase();
            if key.chars().all(|c| c.is_ascii_alphabetic() || c == '_') {
                let nums: Vec<u64> = rest
                    .split_whitespace()
                    .filter_map(|x| x.parse::<u64>().ok())
                    .collect();
                // Per-op mountstats columns (Linux kernel xprt_iostats):
                //   0:ops 1:trans 2:timeouts 3:bytes_sent 4:bytes_recv
                //   5:queue_ms 6:rtt_ms 7:execute_ms 8:errors(optional)
                if nums.len() >= 8 {
                    let calls = nums[0];
                    let bytes_sent = nums[3];
                    let bytes_recv = nums[4];
                    let rtt_ms_total = nums[6] as f64;
                    let exe_ms_total = nums[7] as f64;
                    ops.insert(
                        key.clone(),
                        OpCounters {
                            op: key,
                            calls,
                            bytes_sent,
                            bytes_recv,
                            rtt_ms_total,
                            exe_ms_total,
                        },
                    );
                }
            }
        }
    }

    if fstype != "nfs" && fstype != "nfs4" {
        return None;
    }

    let vers = options.get("vers").cloned();
    let proto = options.get("proto").cloned();
    let nconnect = options.get("nconnect").and_then(|v| v.parse::<u32>().ok());
    let addr = options.get("addr").and_then(|v| parse_ip_maybe(v));
    let clientaddr = options.get("clientaddr").and_then(|v| parse_ip_maybe(v));

    Some(MountCounters {
        device,
        mountpoint,
        fstype,
        vers,
        proto,
        nconnect,
        addr,
        clientaddr,
        options,
        ops,
        raw_block: raw.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::parse_mountstats;

    #[test]
    fn parses_basic_block() {
        // Per-op columns: ops trans timeouts bytes_sent bytes_recv queue_ms rtt_ms exe_ms errors
        let data = "device server:/exp mounted on /mnt with fstype nfs4 statvers=1.1\n opts: rw,vers=4.1,proto=tcp,addr=10.0.0.2,nconnect=4\n per-op statistics\n READ: 10 10 0 280 1048576 5 200 250 0\n WRITE: 5 5 0 524288 140 3 100 130 0\n";
        let mounts = parse_mountstats(data).expect("parse");
        assert_eq!(mounts.len(), 1);
        assert_eq!(mounts[0].mountpoint, "/mnt");
        assert_eq!(mounts[0].nconnect, Some(4));

        let read = mounts[0].ops.get("READ").expect("READ op");
        assert_eq!(read.calls, 10);
        assert_eq!(read.bytes_sent, 280);
        assert_eq!(read.bytes_recv, 1048576);
        assert_eq!(read.rtt_ms_total, 200.0);
        assert_eq!(read.exe_ms_total, 250.0);

        let write = mounts[0].ops.get("WRITE").expect("WRITE op");
        assert_eq!(write.bytes_sent, 524288);
        assert_eq!(write.bytes_recv, 140);
        assert_eq!(write.exe_ms_total, 130.0);
    }
}
