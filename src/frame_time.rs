//! Processing times of frames: a histogram the audio path fills lock-free, one atomic add per
//! frame, from which the report thread reads the mean, percentiles and the longest time of an
//! interval.

use ::std::sync::atomic::{AtomicU64, Ordering};

/// Buckets per doubling of the time: a bucket spans at most 1/64 of its values, so a percentile
/// is rounded up by at most 1.6 %.
const SUB_BITS: u32 = 6;
/// Exact up to 128 ns, then 64 per doubling, up to about 2 s (2^31 ns); longer times land in the
/// last one.
const BUCKETS: usize = ((30 - SUB_BITS as usize) << SUB_BITS) + (2 << SUB_BITS);

/// The processing times of the frames since the last [`FrameTime::publish`], and the summary of
/// the interval before for other readers.
pub struct FrameTime {
    buckets: [AtomicU64; BUCKETS],
    sum_ns: AtomicU64,
    max_ns: AtomicU64,
    last: [AtomicU64; 5],
}

/// The processing times of the frames of an interval, in nanoseconds; the percentiles rounded up
/// by at most 1.6 %, never above `max_ns`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Summary {
    pub frames: u64,
    pub mean_ns: u64,
    pub p95_ns: u64,
    pub p99_ns: u64,
    pub max_ns: u64,
}

impl Default for FrameTime {
    fn default() -> Self {
        Self {
            buckets: ::std::array::from_fn(|_| AtomicU64::new(0)),
            sum_ns: AtomicU64::new(0),
            max_ns: AtomicU64::new(0),
            last: Default::default(),
        }
    }
}

impl FrameTime {
    /// From the audio path: the processing time of a frame.
    pub fn record(&self, ns: u64) {
        self.buckets[bucket(ns)].fetch_add(1, Ordering::Relaxed);
        self.sum_ns.fetch_add(ns, Ordering::Relaxed);
        self.max_ns.fetch_max(ns, Ordering::Relaxed);
    }

    /// Summarizes the frames since the last call, starts a new interval and keeps the summary for
    /// [`Self::last`]. A frame recorded meanwhile may count partly in the next interval.
    pub fn publish(&self) -> Summary {
        let mut counts = [0; BUCKETS];
        for (count, bucket) in counts.iter_mut().zip(&self.buckets) {
            *count = bucket.swap(0, Ordering::Relaxed);
        }
        let sum_ns = self.sum_ns.swap(0, Ordering::Relaxed);
        let max_ns = self.max_ns.swap(0, Ordering::Relaxed);
        let frames: u64 = counts.iter().sum();
        let summary = if frames == 0 {
            Summary::default()
        } else {
            let percentile = |percent: u64| {
                // the frame at this rank, counted from the fastest, 1-based
                let rank = (frames * percent).div_ceil(100);
                let mut seen = 0;
                let index = counts
                    .iter()
                    .position(|&count| {
                        seen += count;
                        seen >= rank
                    })
                    .unwrap_or(BUCKETS - 1);
                upper_edge(index).min(max_ns)
            };
            Summary {
                frames,
                mean_ns: sum_ns / frames,
                p95_ns: percentile(95),
                p99_ns: percentile(99),
                max_ns,
            }
        };
        let values = [
            summary.frames,
            summary.mean_ns,
            summary.p95_ns,
            summary.p99_ns,
            summary.max_ns,
        ];
        for (last, value) in self.last.iter().zip(values) {
            last.store(value, Ordering::Relaxed);
        }
        summary
    }

    /// What the last [`Self::publish`] returned.
    pub fn last(&self) -> Summary {
        let [frames, mean_ns, p95_ns, p99_ns, max_ns] =
            ::std::array::from_fn(|i| self.last[i].load(Ordering::Relaxed));
        Summary {
            frames,
            mean_ns,
            p95_ns,
            p99_ns,
            max_ns,
        }
    }
}

/// The bucket of a time: below 128 ns one per nanosecond, then by the highest bit and the
/// `SUB_BITS` after it.
fn bucket(ns: u64) -> usize {
    if ns < 2 << SUB_BITS {
        return ns as usize;
    }
    let shift = 63 - ns.leading_zeros() - SUB_BITS;
    let index = ((shift as usize) << SUB_BITS) + (ns >> shift) as usize;
    index.min(BUCKETS - 1)
}

/// The longest time that falls into the bucket `index`; the last one takes any.
fn upper_edge(index: usize) -> u64 {
    if index == BUCKETS - 1 {
        return u64::MAX;
    }
    let shift = (index >> SUB_BITS).saturating_sub(1);
    let mantissa = (index - (shift << SUB_BITS)) as u64;
    ((mantissa + 1) << shift) - 1
}
