/*
 * nfs-top eBPF latency probes.
 *
 * Pairs raw_tracepoint probes at the NFS-client layer to measure
 * per-op latency in log2-ns buckets. Userspace folds the histograms
 * into MountDerived.bpf alongside the existing /proc-derived counters;
 * this code never replaces the /proc path.
 *
 * Probes (fire on both NFSv3 and NFSv4 read/write/commit paths):
 *   nfs_initiate_read   / nfs_readpage_done    -> OP_READ
 *   nfs_initiate_write  / nfs_writeback_done   -> OP_WRITE
 *   nfs_initiate_commit / nfs_commit_done      -> OP_COMMIT
 *
 * In-flight key: pointer to nfs_pgio_header (read/write) or
 * nfs_commit_data (commit), which is stable across the init/done pair.
 * No struct-field dereferences in v0, so no module BTF dependency.
 *
 * Histogram key: (op_id, log2(latency_ns)). Userspace deltas the
 * counts each tick (snapshot-and-diff), no map reset.
 */
#include <linux/bpf.h>
#include <bpf/bpf_helpers.h>

#include "nfs_lat.bpf.h"

char LICENSE[] SEC("license") = "GPL";

struct {
	__uint(type, BPF_MAP_TYPE_HASH);
	__uint(max_entries, 1024);
	__type(key, struct hist_key);
	__type(value, __u64);
} hist SEC(".maps");

struct {
	__uint(type, BPF_MAP_TYPE_HASH);
	__uint(max_entries, 65536);
	__type(key, __u64);
	__type(value, struct inflight_val);
} inflight SEC(".maps");

/* floor(log2(ns)) capped at 63. Caller guarantees ns >= 1. */
static __always_inline __u16 log2_bucket(__u64 ns)
{
	if (ns < 2)
		return 0;
	return 63 - __builtin_clzll(ns);
}

static __always_inline int record_start(__u64 key, __u16 op_id)
{
	struct inflight_val v = {};
	v.ts_ns = bpf_ktime_get_ns();
	v.op_id = op_id;
	bpf_map_update_elem(&inflight, &key, &v, BPF_ANY);
	return 0;
}

static __always_inline int record_done(__u64 key)
{
	struct inflight_val *v = bpf_map_lookup_elem(&inflight, &key);
	if (!v)
		return 0;

	__u64 lat = bpf_ktime_get_ns() - v->ts_ns;
	struct hist_key hk = {
		.op_id = v->op_id,
		.bucket = log2_bucket(lat),
	};
	/* Race-safe cold-start: two CPUs landing on a never-seen
	 * (op_id, bucket) both BPF_NOEXIST(zero); whichever loses still
	 * sees the entry on the second lookup, so neither sample is lost. */
	__u64 *cnt = bpf_map_lookup_elem(&hist, &hk);
	if (!cnt) {
		__u64 zero = 0;
		bpf_map_update_elem(&hist, &hk, &zero, BPF_NOEXIST);
		cnt = bpf_map_lookup_elem(&hist, &hk);
		if (!cnt) {
			bpf_map_delete_elem(&inflight, &key);
			return 0;
		}
	}
	__sync_fetch_and_add(cnt, 1);
	bpf_map_delete_elem(&inflight, &key);
	return 0;
}

/* Read: init has 1 arg (hdr); done has 2 args (task, hdr). */
SEC("raw_tracepoint/nfs_initiate_read")
int handle_read_init(struct bpf_raw_tracepoint_args *ctx)
{
	return record_start(ctx->args[0], OP_READ);
}

SEC("raw_tracepoint/nfs_readpage_done")
int handle_read_done(struct bpf_raw_tracepoint_args *ctx)
{
	return record_done(ctx->args[1]);
}

/* Write: init has 1 arg (hdr); done has 2 args (task, hdr). */
SEC("raw_tracepoint/nfs_initiate_write")
int handle_write_init(struct bpf_raw_tracepoint_args *ctx)
{
	return record_start(ctx->args[0], OP_WRITE);
}

SEC("raw_tracepoint/nfs_writeback_done")
int handle_write_done(struct bpf_raw_tracepoint_args *ctx)
{
	return record_done(ctx->args[1]);
}

/* Commit: init has 1 arg (data); done has 2 args (task, data). */
SEC("raw_tracepoint/nfs_initiate_commit")
int handle_commit_init(struct bpf_raw_tracepoint_args *ctx)
{
	return record_start(ctx->args[0], OP_COMMIT);
}

SEC("raw_tracepoint/nfs_commit_done")
int handle_commit_done(struct bpf_raw_tracepoint_args *ctx)
{
	return record_done(ctx->args[1]);
}
