/*
 * Shared types between the BPF programs and the Rust userspace loader.
 *
 * Keep this file dependency-free so it can be included from both BPF C
 * (where stdint.h is unavailable) and Rust (via bindgen, in a follow-up).
 * For now the Rust side mirrors these types by hand in src/sampler/ebpf.rs.
 *
 * SCHEMA v2: hist_key carries the NFS super_block s_dev so userspace can
 * split per-mount. Bumping the layout requires the Rust loader's key
 * decoder to match — keep the offsets in lockstep.
 */
#ifndef NFS_LAT_BPF_H
#define NFS_LAT_BPF_H

#ifndef __bpf__
#include <stdint.h>
typedef uint16_t __u16;
typedef uint32_t __u32;
typedef uint64_t __u64;
#endif

/* Operation identifiers. Keep in sync with the OP_* consts in
 * src/sampler/ebpf.rs and the matching arms in op_name(). Names mirror
 * what mountstats prints so users can cross-reference Hist-tab rows
 * against /proc/self/mountstats. OP_FSYNC and OP_OPEN have no
 * mountstats counterpart (OPEN is NFSv4-only); they're harmless on
 * mounts where the underlying tracepoint never fires. ID 0 is left
 * unused so a zero-initialized key is unambiguously invalid. */
enum nfs_op_id {
	OP_READ    = 1,
	OP_WRITE   = 2,
	OP_COMMIT  = 3,
	OP_GETATTR = 4,
	OP_SETATTR = 5,
	OP_LOOKUP  = 6,
	OP_ACCESS  = 7,
	OP_CREATE  = 8,
	OP_REMOVE  = 9,
	OP_RENAME  = 10,
	OP_LINK    = 11,
	OP_SYMLINK = 12,
	OP_MKDIR   = 13,
	OP_RMDIR   = 14,
	OP_MKNOD   = 15,
	OP_FSYNC   = 16,
	OP_OPEN    = 17,
};

#define NFS_OP_MAX 18

/* Histogram key: (dev, op, log2_bucket). bucket = floor(log2(latency_ns)).
 * `dev` is the kernel-side super_block.s_dev (MKDEV(major, minor)); 0 means
 * the init probe could not resolve the inode chain. */
struct hist_key {
	__u32 dev;
	__u16 op_id;
	__u16 bucket;
};

/* In-flight bookkeeping: one entry per in-progress NFS op. dev is captured
 * at init time so the done probe doesn't have to re-walk the struct chain. */
struct inflight_val {
	__u64 ts_ns;
	__u32 dev;
	__u16 op_id;
	__u16 _pad;
};

#endif /* NFS_LAT_BPF_H */
