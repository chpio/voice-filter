mod common;

use ::std::sync::{Mutex, MutexGuard};

use ::voice_filter::bridge::bridge;
use ::voice_filter::rt_alloc::{POOL_SIZE, PoolStats, RtAlloc, audio_path};
use ::voice_filter::{FRAME_LEN, SAMPLE_RATE, chain};
use common::{Noise, vowel};

#[global_allocator]
static ALLOC: RtAlloc = RtAlloc::new();

/// The tests share the pool and its counters, so they run one after the other.
fn serial() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[test]
fn audio_path_allocates_from_the_pool() {
    let _serial = serial();
    let outside = Box::new(1u64);
    let inside = audio_path(|| Box::new(2u64));
    assert!(!ALLOC.contains(&raw const *outside as *const u8));
    assert!(ALLOC.contains(&raw const *inside as *const u8));
    // nested, and back outside after it
    let nested = audio_path(|| {
        audio_path(|| ());
        Box::new(3u64)
    });
    assert!(ALLOC.contains(&raw const *nested as *const u8));
    let after = Box::new(4u64);
    assert!(!ALLOC.contains(&raw const *after as *const u8));
    assert_eq!((*outside, *inside, *nested, *after), (1, 2, 3, 4));
}

#[test]
fn frees_go_back_where_they_came_from() {
    let _serial = serial();
    let before = ALLOC.stats();
    // freed outside the audio path, and on another thread
    let inside = audio_path(|| vec![7u8; 1000]);
    drop(inside);
    let inside = audio_path(|| vec![8u8; 1000]);
    ::std::thread::spawn(move || drop(inside)).join().unwrap();
    // with an alignment above TLSF's granularity, as tract's tensors have
    let aligned = audio_path(|| Box::new(Aligned([9; 256])));
    assert_eq!(aligned.0.as_ptr() as usize % 128, 0);
    drop(aligned);
    let after = ALLOC.stats();
    assert_eq!(after.used, before.used);
    assert_eq!(after.outside, before.outside);
}

#[repr(align(128))]
struct Aligned([u8; 256]);

#[test]
fn realloc_moves_between_pool_and_system() {
    let _serial = serial();
    let before = ALLOC.stats();
    // from the system allocator into the pool: freeing the old block counts as outside
    let mut grown: Vec<u32> = (0..100).collect();
    audio_path(|| grown.extend(100..10_000));
    assert!(ALLOC.contains(grown.as_ptr().cast()));
    // within the pool
    audio_path(|| grown.extend(10_000..100_000));
    assert!(ALLOC.contains(grown.as_ptr().cast()));
    // out of it again
    grown.extend(100_000..1_000_000);
    assert!(!ALLOC.contains(grown.as_ptr().cast()));
    assert!(grown.iter().copied().eq(0..1_000_000));
    drop(grown);
    let after = ALLOC.stats();
    assert_eq!(after.used, before.used);
    assert_eq!(after.outside, before.outside + 1);
}

#[test]
fn what_does_not_fit_goes_to_the_system_allocator() {
    let _serial = serial();
    let before = ALLOC.stats().outside;
    let large = audio_path(|| vec![1u8; POOL_SIZE + 1]);
    assert!(!ALLOC.contains(large.as_ptr()));
    assert_eq!(ALLOC.stats().outside, before + 1);
    // freeing it in the audio path goes to the system allocator too
    audio_path(|| drop(large));
    assert_eq!(ALLOC.stats().outside, before + 2);
}

/// `passes` times 2 s of a voice at three levels over noise, through the chain as the live mode
/// runs it, then the stats of the pool.
fn run_chain(passes: usize) -> Vec<PoolStats> {
    let (chain, _reference) = chain::build(None).unwrap();
    let (mut capture, mut playback, _stats) = bridge(chain);
    let mut noise = Noise(0x2545_f491);
    let len = 2 * SAMPLE_RATE as usize;
    let input: Vec<f32> = (0..len)
        .map(|n| {
            let level = [1., 0.3, 0.05][n * 6 / len % 3];
            level * vowel(n) + 0.003 * noise.sample()
        })
        .collect();
    let mut output = [0.; FRAME_LEN];
    (0..passes)
        .map(|_| {
            for (i, frame) in input.chunks(FRAME_LEN).enumerate() {
                let now = (i * FRAME_LEN) as i64 * 1_000_000_000 / SAMPLE_RATE as i64;
                capture.push(frame, now);
                playback.pull(&mut output, now);
            }
            ALLOC.stats()
        })
        .collect()
}

/// The chain allocates per frame (tract runs the model, see `rt_alloc`): in the audio path all of
/// it comes from the pool and goes back to it, so after the first pass the pool doesn't grow.
#[test]
fn the_chain_runs_on_the_pool() {
    let _serial = serial();
    let before = ALLOC.stats();
    let stats = run_chain(3);
    eprintln!("pool after each pass: {stats:?}");
    for pass in &stats {
        assert_eq!(pass.outside, before.outside, "{pass:?}");
        assert!(pass.peak < POOL_SIZE / 2, "{pass:?}");
    }
    assert_eq!(stats[2].used, stats[1].used);
}
