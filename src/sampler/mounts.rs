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

/// Read NFS mount → kernel `s_dev` mapping from `/proc/self/mountinfo`.
///
/// `/proc/self/mountstats` (the throughput/latency source) does not carry
/// the super_block device id, but `mountinfo` does — column 3 is
/// `major:minor`. We pack those into the same 32-bit form the kernel uses
/// (`MKDEV`) so the value matches what BPF reads off the inode chain.
pub fn read_mount_devs() -> Result<HashMap<String, u32>> {
    let data = fs::read_to_string("/proc/self/mountinfo")
        .context("reading /proc/self/mountinfo")?;
    Ok(parse_mountinfo_devs(&data))
}

/// Linux's in-kernel MKDEV: 12-bit major in the high bits, 20-bit minor.
/// Matches the layout of `super_block.s_dev` that BPF probes read.
fn pack_dev(major: u32, minor: u32) -> u32 {
    ((major & 0xfff) << 20) | (minor & 0xfffff)
}

fn parse_mountinfo_devs(data: &str) -> HashMap<String, u32> {
    let mut out = HashMap::new();
    for line in data.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        // mountinfo: [mount_id parent_id major:minor root mountpoint
        //             mount_opts (optional…) "-" fstype source super_opts]
        let Some(sep) = cols.iter().position(|c| *c == "-") else {
            continue;
        };
        if cols.len() < 5 || cols.len() < sep + 2 {
            continue;
        }
        let fstype = cols[sep + 1];
        if fstype != "nfs" && fstype != "nfs4" {
            continue;
        }
        let Some((maj, min)) = cols[2].split_once(':') else {
            continue;
        };
        let (Ok(major), Ok(minor)) = (maj.parse::<u32>(), min.parse::<u32>()) else {
            continue;
        };
        out.insert(cols[4].to_string(), pack_dev(major, minor));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_dev_matches_kernel_mkdev() {
        // Anonymous-block NFS major=0, minor=51 (typical) → 51.
        assert_eq!(pack_dev(0, 51), 51);
        // Local block major=253, minor=0 → 253 << 20 = 265289728. Matches
        // what stat() returns and what super_block.s_dev holds.
        assert_eq!(pack_dev(253, 0), 265_289_728);
    }

    #[test]
    fn parses_nfs_lines_only() {
        let data = "\
36 35 0:51 / /mnt/a rw,relatime shared:1 - nfs server:/exp rw,vers=4
37 35 0:52 / /mnt/b rw,relatime shared:2 - nfs4 server:/exp2 rw,vers=4
38 35 8:1 / /home rw,relatime shared:3 - ext4 /dev/sda1 rw
";
        let m = parse_mountinfo_devs(data);
        assert_eq!(m.len(), 2);
        assert_eq!(m.get("/mnt/a"), Some(&51));
        assert_eq!(m.get("/mnt/b"), Some(&52));
        assert!(!m.contains_key("/home"));
    }

    #[test]
    fn handles_optional_fields_before_separator() {
        // Optional propagation fields (shared:, master:, propagate_from:)
        // sit between mount_opts and "-". The parser must still find "-".
        let data = "100 99 0:99 / /mnt/x rw shared:5 master:6 - nfs s:/e rw\n";
        let m = parse_mountinfo_devs(data);
        assert_eq!(m.get("/mnt/x"), Some(&99));
    }
}
