//! The allocator of the binary: what the audio path allocates comes from a pool reserved up front.
//!
//! DeepFilterNet3 runs on tract, which allocates its outputs and packed operands for every node,
//! about 820 times per frame, and has no way to run on preallocated tensors. The system allocator
//! takes locks and may go to the kernel for that, so allocations within [`audio_path`] go to a
//! pool of [`POOL_SIZE`] instead: a TLSF allocator (`rlsf`, constant time) over a static block of
//! memory that [`RtAlloc::init`] touches once, so its pages are mapped before the first frame.
//! Everything else goes to the system allocator, and what's freed goes back to where it came from
//! (by its address), on any thread.
//!
//! A spin lock guards the pool: the audio path is normally its only user, other threads only take
//! it to free what came from there (e.g. a chain dropped at the end). What the pool can't take
//! goes to the system allocator, counted in [`PoolStats::outside`] along with what the audio path
//! frees there; `tests/rt_alloc.rs` checks it stays at 0 for the chain.

use ::std::alloc::{GlobalAlloc, Layout, System};
use ::std::cell::{Cell, UnsafeCell};
use ::std::hint;
use ::std::mem::MaybeUninit;
use ::std::ptr::{self, NonNull};
use ::std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU64, AtomicUsize, Ordering};

use ::rlsf::Tlsf;

/// The size of the pool in bytes: the chain takes about 300 KB at most, 150 KB of it for good
/// (`tests/rt_alloc.rs` prints it).
pub const POOL_SIZE: usize = 4 << 20;

/// The free lists of the pool: blocks of up to 32 MiB (TLSF's granularity of 32 bytes, shifted by
/// 20), each power of two split into 32 lists.
type Pool = Tlsf<'static, u32, u32, 20, 32>;

thread_local! {
    /// Whether the current thread is in [`audio_path`].
    static AUDIO_PATH: Cell<bool> = const { Cell::new(false) };
}

/// Runs `f` as the audio path: what it allocates comes from the pool of the global [`RtAlloc`]
/// (without one, it's just `f`).
pub fn audio_path<R>(f: impl FnOnce() -> R) -> R {
    /// Restores the flag, also when `f` unwinds.
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            AUDIO_PATH.set(self.0);
        }
    }
    let _restore = Restore(AUDIO_PATH.replace(true));
    f()
}

fn in_audio_path() -> bool {
    // a `const` thread local without a destructor is there as long as the thread
    AUDIO_PATH.get()
}

/// The global allocator: the system allocator, but within [`audio_path`] the pool.
pub struct RtAlloc {
    memory: UnsafeCell<MaybeUninit<[u8; POOL_SIZE]>>,
    // guarded by `locked`, as is `ready`
    pool: UnsafeCell<Pool>,
    ready: UnsafeCell<bool>,
    locked: AtomicBool,
    /// The bytes allocated from the pool, and the most there were.
    used: AtomicUsize,
    peak: AtomicUsize,
    outside: AtomicU64,
}

// SAFETY: `pool` and `ready` are only accessed with the lock held, `memory` only through the pool
unsafe impl Sync for RtAlloc {}

/// The [`RtAlloc`] that [`RtAlloc::init`] was called on, for [`stats`].
static ACTIVE: AtomicPtr<RtAlloc> = AtomicPtr::new(ptr::null_mut());

/// How the pool is used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PoolStats {
    /// Its size, [`POOL_SIZE`].
    pub size: usize,
    /// The bytes allocated from it now.
    pub used: usize,
    /// The most bytes allocated from it at a time.
    pub peak: usize,
    /// The calls of the audio path that went to the system allocator: allocations that didn't fit
    /// into the pool and frees of memory that came from the system allocator.
    pub outside: u64,
}

/// The pool with its lock held.
struct Locked<'a>(&'a RtAlloc);

impl Locked<'_> {
    fn pool(&mut self) -> &mut Pool {
        let alloc = self.0;
        // SAFETY: the lock is held
        unsafe {
            if !*alloc.ready.get() {
                alloc.prepare();
                *alloc.ready.get() = true;
            }
            &mut *alloc.pool.get()
        }
    }
}

impl Drop for Locked<'_> {
    fn drop(&mut self) {
        self.0.locked.store(false, Ordering::Release);
    }
}

impl RtAlloc {
    #[allow(clippy::new_without_default)] // for a `static`, where `Default` can't be called
    pub const fn new() -> Self {
        Self {
            memory: UnsafeCell::new(MaybeUninit::uninit()),
            pool: UnsafeCell::new(Tlsf::new()),
            ready: UnsafeCell::new(false),
            locked: AtomicBool::new(false),
            used: AtomicUsize::new(0),
            peak: AtomicUsize::new(0),
            outside: AtomicU64::new(0),
        }
    }

    /// Maps the pool's pages, so the audio path doesn't fault them in, and makes it the one
    /// [`stats`] reports. Without it, the first allocation of the audio path does the same.
    pub fn init(&'static self) {
        self.lock().pool();
        ACTIVE.store(ptr::from_ref(self).cast_mut(), Ordering::Release);
    }

    pub fn stats(&self) -> PoolStats {
        PoolStats {
            size: POOL_SIZE,
            used: self.used.load(Ordering::Relaxed),
            peak: self.peak.load(Ordering::Relaxed),
            outside: self.outside.load(Ordering::Relaxed),
        }
    }

    /// Whether `ptr` points into the pool.
    pub fn contains(&self, ptr: *const u8) -> bool {
        (ptr as usize).wrapping_sub(self.memory.get() as usize) < POOL_SIZE
    }

    fn lock(&self) -> Locked<'_> {
        while self
            .locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            hint::spin_loop();
        }
        Locked(self)
    }

    /// Writes the whole memory, then hands it to the pool.
    ///
    /// # Safety
    ///
    /// The lock must be held, and it must be the first call.
    unsafe fn prepare(&self) {
        let memory = self.memory.get().cast::<u8>();
        // SAFETY: within `memory`, which nothing uses yet; volatile, so it isn't left out
        unsafe {
            for offset in (0..POOL_SIZE).step_by(4096) {
                memory.add(offset).write_volatile(0);
            }
            let block = NonNull::slice_from_raw_parts(NonNull::new_unchecked(memory), POOL_SIZE);
            (*self.pool.get()).insert_free_block_ptr(block);
        }
    }

    fn pool_alloc(&self, layout: Layout) -> Option<*mut u8> {
        let ptr = self.lock().pool().allocate(layout)?;
        let used = self.used.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
        self.peak.fetch_max(used, Ordering::Relaxed);
        Some(ptr.as_ptr())
    }

    /// # Safety
    ///
    /// `ptr` must have come from the pool with `layout`.
    unsafe fn pool_dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: as the caller guarantees
        unsafe {
            self.lock()
                .pool()
                .deallocate(NonNull::new_unchecked(ptr), layout.align())
        };
        self.used.fetch_sub(layout.size(), Ordering::Relaxed);
    }

    fn count_outside(&self) {
        self.outside.fetch_add(1, Ordering::Relaxed);
    }
}

/// The stats of the [`RtAlloc`] that was initialized, if there is one.
pub fn stats() -> Option<PoolStats> {
    // SAFETY: only ever set to a `&'static RtAlloc`
    unsafe { ACTIVE.load(Ordering::Acquire).as_ref() }.map(RtAlloc::stats)
}

// SAFETY: the pool hands out blocks of the layout asked for (or none), the system allocator the
// rest; every block goes back to where it came from, decided by its address
unsafe impl GlobalAlloc for RtAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if in_audio_path() {
            if let Some(ptr) = self.pool_alloc(layout) {
                return ptr;
            }
            self.count_outside();
        }
        // SAFETY: the caller's guarantees hold for the system allocator too
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if in_audio_path() {
            if let Some(ptr) = self.pool_alloc(layout) {
                // SAFETY: the block was just allocated with this size
                unsafe { ptr.write_bytes(0, layout.size()) };
                return ptr;
            }
            self.count_outside();
        }
        // SAFETY: as in `alloc`
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if self.contains(ptr) {
            // SAFETY: it's in the pool, so it came from there, with this layout as the caller
            // guarantees
            unsafe { self.pool_dealloc(ptr, layout) };
        } else {
            if in_audio_path() {
                self.count_outside();
            }
            // SAFETY: not from the pool, so from the system allocator
            unsafe { System.dealloc(ptr, layout) };
        }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let audio = in_audio_path();
        let from_pool = self.contains(ptr);
        if !from_pool && !audio {
            // SAFETY: as in `dealloc`
            return unsafe { System.realloc(ptr, layout, new_size) };
        }
        // SAFETY: the caller guarantees a valid size for the alignment
        let new_layout = unsafe { Layout::from_size_align_unchecked(new_size, layout.align()) };
        if from_pool && audio {
            // SAFETY: from the pool with this alignment
            let moved = unsafe {
                self.lock()
                    .pool()
                    .reallocate(NonNull::new_unchecked(ptr), new_layout)
            };
            if let Some(moved) = moved {
                self.used.fetch_sub(layout.size(), Ordering::Relaxed);
                let used = self.used.fetch_add(new_size, Ordering::Relaxed) + new_size;
                self.peak.fetch_max(used, Ordering::Relaxed);
                return moved.as_ptr();
            }
        }
        // between the pool and the system allocator, or the pool is full: a new block where
        // `alloc` puts it, then the old one freed where it came from
        // SAFETY: `new_layout` has a size above 0, as the caller guarantees
        let new = unsafe { self.alloc(new_layout) };
        if !new.is_null() {
            // SAFETY: both blocks hold at least this many bytes and don't overlap
            unsafe {
                ptr::copy_nonoverlapping(ptr, new, layout.size().min(new_size));
                self.dealloc(ptr, layout);
            }
        }
        new
    }
}
