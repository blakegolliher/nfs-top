//! Log2-bucket histogram percentile math.
//!
//! Buckets are powers of two in nanoseconds: bucket `i` covers
//! `[2^i, 2^(i+1))`. As a special case, the BPF binner folds ns=0 into
//! bucket 0, so bucket 0's effective range is `[0, 2)`. The reported
//! percentile is the **upper edge** of the bucket containing the target
//! rank, so the true value is at most that number. This matches the
//! HDR-histogram convention and gives a worst-case bound suitable for
//! tail-latency claims.

pub const BUCKETS: usize = 64;

/// Upper edge of bucket `i`, in nanoseconds. Bucket 0 covers `[0, 2)`
/// (the binner extends `[2^0, 2^1)` to also include ns=0), bucket 1
/// covers `[2, 4)`, bucket 63 covers `[2^63, u64::MAX]`.
pub fn bucket_upper_ns(i: usize) -> u64 {
    if i >= 63 {
        u64::MAX
    } else {
        1u64 << (i + 1)
    }
}

/// Walk the histogram in ascending bucket order, returning the upper edge
/// of the first bucket whose cumulative count reaches `ceil(total * p)`.
/// Returns 0 if `total == 0`.
pub fn percentile_ns(buckets: &[u64; BUCKETS], total: u64, p: f64) -> u64 {
    if total == 0 {
        return 0;
    }
    // ceil(total * p), clamped so p=1.0 lands on the last sample.
    let target = ((total as f64) * p).ceil().max(1.0) as u64;
    let target = target.min(total);
    let mut acc: u64 = 0;
    for (i, &c) in buckets.iter().enumerate() {
        acc = acc.saturating_add(c);
        if acc >= target {
            return bucket_upper_ns(i);
        }
    }
    bucket_upper_ns(BUCKETS - 1)
}

/// Highest bucket with any samples, as an upper-edge ns value. 0 if empty.
pub fn max_ns(buckets: &[u64; BUCKETS]) -> u64 {
    for i in (0..BUCKETS).rev() {
        if buckets[i] > 0 {
            return bucket_upper_ns(i);
        }
    }
    0
}

/// Total sample count.
pub fn total(buckets: &[u64; BUCKETS]) -> u64 {
    buckets.iter().fold(0u64, |a, b| a.saturating_add(*b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_hist_is_zero() {
        let b = [0u64; BUCKETS];
        assert_eq!(total(&b), 0);
        assert_eq!(max_ns(&b), 0);
        assert_eq!(percentile_ns(&b, 0, 0.5), 0);
    }

    #[test]
    fn percentile_picks_smallest_bucket_meeting_rank() {
        // 100 samples in bucket 10 (1024..2048 ns) — every percentile
        // up to 1.0 must land at the upper edge 2048.
        let mut b = [0u64; BUCKETS];
        b[10] = 100;
        let t = total(&b);
        assert_eq!(t, 100);
        assert_eq!(percentile_ns(&b, t, 0.5), 2048);
        assert_eq!(percentile_ns(&b, t, 0.99), 2048);
        assert_eq!(percentile_ns(&b, t, 1.0), 2048);
        assert_eq!(max_ns(&b), 2048);
    }

    #[test]
    fn percentile_walks_across_buckets() {
        // 90 in bucket 10 (~1us), 9 in bucket 13 (~8us), 1 in bucket 20 (~1ms).
        let mut b = [0u64; BUCKETS];
        b[10] = 90;
        b[13] = 9;
        b[20] = 1;
        let t = total(&b);
        assert_eq!(t, 100);
        assert_eq!(percentile_ns(&b, t, 0.50), 2048); // bucket 10 upper
        assert_eq!(percentile_ns(&b, t, 0.90), 2048);
        assert_eq!(percentile_ns(&b, t, 0.99), 16384); // bucket 13 upper
        assert_eq!(percentile_ns(&b, t, 0.999), 2_097_152); // bucket 20 upper
        assert_eq!(max_ns(&b), 2_097_152);
    }

    #[test]
    fn last_bucket_clamps_to_u64_max() {
        let mut b = [0u64; BUCKETS];
        b[63] = 5;
        assert_eq!(bucket_upper_ns(63), u64::MAX);
        assert_eq!(percentile_ns(&b, 5, 1.0), u64::MAX);
    }
}
