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

/* Operation identifiers. Keep in sync with OP_NAMES in src/sampler/ebpf.rs. */
enum nfs_op_id {
	OP_OTHER  = 0,
	OP_READ   = 1,
	OP_WRITE  = 2,
	OP_COMMIT = 3,
};

#define NFS_OP_MAX 4

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
