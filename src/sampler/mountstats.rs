use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;

use crate::model::types::{MountCounters, OpCounters};
use crate::util::parse::{parse_ip_maybe, parse_kv_options};

const MOUNTSTATS_PATH: &str = "/proc/self/mountstats";

pub fn read_mountstats() -> Result<Vec<MountCounters>> {
    let raw = fs::read_to_string(MOUNTSTATS_PATH)
        .with_context(|| format!("reading {MOUNTSTATS_PATH}"))?;
    parse_mountstats(&raw)
}

pub fn parse_mountstats(input: &str) -> Result<Vec<MountCounters>> {
    // Track each mount's block as a byte-range slice into `input` instead of
    // collecting owned String lines and re-joining them. The original code
    // allocated O(lines) Strings per tick on a host with many mounts.
    let mut out = Vec::new();
    let mut block_start: Option<usize> = None;
    let mut offset = 0usize;

    for line_with_term in input.split_inclusive('\n') {
        let line = line_with_term.strip_suffix('\n').unwrap_or(line_with_term);
        if line.starts_with("device ") {
            if let Some(start) = block_start
                && let Some(m) = parse_block(&input[start..offset])
            {
                out.push(m);
            }
            block_start = Some(offset);
        }
        offset += line_with_term.len();
    }
    if let Some(start) = block_start
        && let Some(m) = parse_block(&input[start..])
    {
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
    let mut in_per_op = false;

    for line in raw.lines() {
        if line.starts_with("device ") {
            let cols: Vec<&str> = line.split_whitespace().collect();
            if cols.len() >= 8 {
                device = cols[1].to_string();
                mountpoint = cols[4].to_string();
                fstype = cols[7].to_string();
            }
            continue;
        }

        if line.contains("opts:") {
            let rhs = line.split_once("opts:").map(|x| x.1).unwrap_or("");
            options.extend(parse_kv_options(rhs));
            continue;
        }

        if line.contains("age:") || line.contains("caps:") {
            options.extend(parse_kv_options(line));
            continue;
        }

        let t = line.trim();
        if t == "per-op statistics" {
            in_per_op = true;
            continue;
        }

        if !in_per_op {
            continue;
        }

        // Per-op mountstats columns (Linux kernel xprt_iostats):
        //   0:ops 1:trans 2:timeouts 3:bytes_sent 4:bytes_recv
        //   5:queue_ms 6:rtt_ms 7:execute_ms 8:errors(optional)
        if let Some((op, rest)) = t.split_once(':') {
            let key = op.trim().to_uppercase();
            if !key.chars().all(|c| c.is_ascii_alphabetic() || c == '_') {
                continue;
            }
            let nums: Vec<u64> = rest
                .split_whitespace()
                .filter_map(|x| x.parse::<u64>().ok())
                .collect();
            if nums.len() >= 8 {
                ops.insert(
                    key.clone(),
                    OpCounters {
                        op: key,
                        calls: nums[0],
                        bytes_sent: nums[3],
                        bytes_recv: nums[4],
                        rtt_ms_total: nums[6] as f64,
                        exe_ms_total: nums[7] as f64,
                    },
                );
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
        st_dev: None,
        options,
        ops,
        raw_block: raw.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::parse_mountstats;

    // Per-op columns: ops trans timeouts bytes_sent bytes_recv queue_ms rtt_ms exe_ms errors
    const READ_OP: &str = " READ: 10 10 0 280 1048576 5 200 250 0";
    const WRITE_OP: &str = " WRITE: 5 5 0 524288 140 3 100 130 0";

    fn block(device: &str, mount: &str, body: &str) -> String {
        format!(
            "device {device} mounted on {mount} with fstype nfs4 statvers=1.1\n\
             opts: rw,vers=4.1,proto=tcp,addr=10.0.0.2,nconnect=4\n\
             {body}",
        )
    }

    #[test]
    fn parses_basic_block() {
        let data = block("server:/exp", "/mnt", &format!(" per-op statistics\n{READ_OP}\n{WRITE_OP}\n"));
        let mounts = parse_mountstats(&data).expect("parse");
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

    #[test]
    fn pre_per_op_lines_do_not_pollute_ops() {
        // events:/bytes:/xprt: lines have >=8 numeric columns and would otherwise
        // be misclassified as ops EVENTS/BYTES/XPRT. Only entries after the
        // "per-op statistics" header should land in the ops map.
        let body = format!(
            " events: 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0\n\
             \x20bytes: 1 2 3 4 5 6 7 8\n\
             \x20xprt: tcp 0 0 1 0 100 100 0 100 0 100 0 0 0\n\
             \x20per-op statistics\n\
             {READ_OP}\n"
        );
        let data = block("s:/e", "/mnt", &body);
        let mounts = parse_mountstats(&data).expect("parse");
        let ops = &mounts[0].ops;
        assert_eq!(ops.len(), 1, "only READ should be in ops, got {:?}", ops.keys().collect::<Vec<_>>());
        assert!(ops.contains_key("READ"));
        assert!(!ops.contains_key("EVENTS"));
        assert!(!ops.contains_key("BYTES"));
        assert!(!ops.contains_key("XPRT"));
    }

    #[test]
    fn parses_multiple_mount_blocks() {
        let body = format!(" per-op statistics\n{READ_OP}\n");
        let data = format!("{}{}", block("a:/x", "/m1", &body), block("b:/y", "/m2", &body));
        let mounts = parse_mountstats(&data).expect("parse");
        assert_eq!(mounts.len(), 2);
        assert_eq!(mounts[0].mountpoint, "/m1");
        assert_eq!(mounts[1].mountpoint, "/m2");
    }

    #[test]
    fn skips_non_nfs_fstypes() {
        let data = "device tmpfs mounted on /tmp with fstype tmpfs statvers=1.1\n opts: rw\n";
        let mounts = parse_mountstats(data).expect("parse");
        assert!(mounts.is_empty());
    }
}
