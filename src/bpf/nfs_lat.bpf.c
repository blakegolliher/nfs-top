/*
 * nfs-top eBPF latency probes.
 *
 * v0 scaffold: defines the histogram and in-flight maps and a stub
 * raw_tracepoint program so the skeleton loads and the verifier accepts
 * the maps. Real probes (nfs_initiate_read/done, write, commit) land in
 * the next commit. Keeping this commit minimal lets the Rust build chain
 * + libbpf-cargo + clang BPF target be validated in isolation.
 */
#include <linux/bpf.h>
#include <bpf/bpf_helpers.h>

#include "nfs_lat.bpf.h"

char LICENSE[] SEC("license") = "GPL";

/* Histogram of latency: (op_id, log2_bucket) -> count.
 * Cardinality bound: NFS_OP_MAX * ~30 active buckets = ~120 entries. */
struct {
	__uint(type, BPF_MAP_TYPE_HASH);
	__uint(max_entries, 1024);
	__type(key, struct hist_key);
	__type(value, __u64);
} hist SEC(".maps");

/* In-flight bookkeeping, keyed by the request pointer (e.g.
 * struct nfs_pgio_header *) which uniquely identifies an op between
 * its initiate and its done tracepoint. */
struct {
	__uint(type, BPF_MAP_TYPE_HASH);
	__uint(max_entries, 65536);
	__type(key, __u64);
	__type(value, struct inflight_val);
} inflight SEC(".maps");

/* Stub program. Attached to a tracepoint that always exists so the
 * skeleton loads cleanly during scaffolding. The body is intentionally
 * empty — replaced by real NFS probes in the next commit. */
SEC("raw_tracepoint/sys_enter")
int handle_stub(void *ctx)
{
	return 0;
}
