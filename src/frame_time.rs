//! Processing times of frames: a histogram the audio path fills lock-free, one atomic add per
//! frame ([`FrameTime`]), and a window over the last seconds of it the report thread keeps
//! ([`Window`]), from which it reads the mean, percentiles and the longest time.

use ::std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

/// Buckets per doubling of the time: a bucket spans at most 1/64 of its values, so a percentile
/// is rounded up by at most 1.6 %.
const SUB_BITS: u32 = 6;
/// Exact up to 128 ns, then 64 per doubling, up to about 2 s (2^31 ns); longer times land in the
/// last one.
const BUCKETS: usize = ((30 - SUB_BITS as usize) << SUB_BITS) + (2 << SUB_BITS);

/// The seconds a [`Window`] spans.
pub const WINDOW_SECONDS: usize = 10;

/// The processing times of the frames since the last [`Window::push`], and the summary of the
/// window at it for other readers.
pub struct FrameTime {
    buckets: [AtomicU32; BUCKETS],
    sum_ns: AtomicU64,
    max_ns: AtomicU64,
    last: [AtomicU64; 5],
}

/// The processing times of the frames of a window, in nanoseconds; the percentiles rounded up by
/// at most 1.6 %, never above `max_ns`.
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
            buckets: ::std::array::from_fn(|_| AtomicU32::new(0)),
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

    /// What the last [`Window::push`] returned.
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

/// The last [`WINDOW_SECONDS`] pushes of a [`FrameTime`], in the thread that pushes them: the
/// counts of every push and their sum, so a push only takes the oldest out and the new one in.
/// Allocated once, a push doesn't allocate.
pub struct Window {
    /// The counts of the buckets per push, the oldest at `next`; a push of more than `u16::MAX`
    /// frames in a bucket keeps that many (100 frames run per second).
    pushes: Box<[[u16; BUCKETS]]>,
    sums_ns: [u64; WINDOW_SECONDS],
    maxes_ns: [u64; WINDOW_SECONDS],
    /// The counts of all of `pushes` per bucket.
    counts: Box<[u32; BUCKETS]>,
    next: usize,
}

impl Default for Window {
    fn default() -> Self {
        Self {
            pushes: vec![[0; BUCKETS]; WINDOW_SECONDS].into_boxed_slice(),
            sums_ns: [0; WINDOW_SECONDS],
            maxes_ns: [0; WINDOW_SECONDS],
            counts: Box::new([0; BUCKETS]),
            next: 0,
        }
    }
}

impl Window {
    /// Takes the frames recorded in `time` since the last push (starting it over) into the window
    /// in place of the oldest push, summarizes the window and keeps that for
    /// [`FrameTime::last`]. A frame recorded meanwhile may count partly in the next push.
    pub fn push(&mut self, time: &FrameTime) -> Summary {
        let slot = self.next;
        self.next = (slot + 1) % WINDOW_SECONDS;
        let push = &mut self.pushes[slot];
        // three loops: the ones over plain arrays vectorize, the atomics don't
        for (count, &old) in self.counts.iter_mut().zip(push.iter()) {
            *count -= u32::from(old);
        }
        for (new, bucket) in push.iter_mut().zip(&time.buckets) {
            *new = bucket.swap(0, Ordering::Relaxed).min(u16::MAX.into()) as u16;
        }
        for (count, &new) in self.counts.iter_mut().zip(push.iter()) {
            *count += u32::from(new);
        }
        self.sums_ns[slot] = time.sum_ns.swap(0, Ordering::Relaxed);
        self.maxes_ns[slot] = time.max_ns.swap(0, Ordering::Relaxed);

        let summary = self.summary();
        let values = [
            summary.frames,
            summary.mean_ns,
            summary.p95_ns,
            summary.p99_ns,
            summary.max_ns,
        ];
        for (last, value) in time.last.iter().zip(values) {
            last.store(value, Ordering::Relaxed);
        }
        summary
    }

    fn summary(&self) -> Summary {
        let frames: u64 = self.counts.iter().map(|&count| u64::from(count)).sum();
        if frames == 0 {
            return Summary::default();
        }
        let max_ns = self.maxes_ns.iter().copied().max().unwrap_or_default();
        let percentile = |percent: u64| {
            // the frame at this rank, counted from the fastest, 1-based
            let rank = (frames * percent).div_ceil(100);
            let mut seen = 0;
            let index = self
                .counts
                .iter()
                .position(|&count| {
                    seen += u64::from(count);
                    seen >= rank
                })
                .unwrap_or(BUCKETS - 1);
            upper_edge(index).min(max_ns)
        };
        Summary {
            frames,
            mean_ns: self.sums_ns.iter().sum::<u64>() / frames,
            p95_ns: percentile(95),
            p99_ns: percentile(99),
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
