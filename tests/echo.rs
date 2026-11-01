use ::std::alloc::{GlobalAlloc, Layout, System};
use ::std::cell::Cell;

use ::voice_filter::FRAME_LEN;
use ::voice_filter::echo::{Echo, LATENCY, Reference, SILENCE_FRAMES};
use ::voice_filter::stage::{Frame, Stage};

/// Counts the allocations of the current thread, so the test harness's other threads don't
/// count.
struct Counting;

thread_local! {
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

// SAFETY: forwards to the system allocator, only counting
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.with(|count| count.set(count.get() + 1));
        // SAFETY: the caller's guarantees hold for the system allocator too
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: as above
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

/// White noise from a fixed seed, at about -24 dBFS.
struct Noise(u32);

impl Noise {
    fn frame(&mut self) -> Frame {
        ::std::array::from_fn(|_| {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 17;
            self.0 ^= self.0 << 5;
            (self.0 as f32 / u32::MAX as f32 * 2. - 1.) * 0.1
        })
    }
}

fn energy(frame: &[f32]) -> f64 {
    frame.iter().map(|&x| (x as f64).powi(2)).sum()
}

/// A room: the echo is the played signal 4 ms later, at a third, plus a weaker reflection.
struct Room {
    played: Vec<f32>,
}

impl Room {
    const DELAY: usize = 192;
    const REFLECTION: usize = 700;

    /// What the microphone picks up of `frame`, played now.
    fn echo(&mut self, frame: &Frame) -> Frame {
        self.played.extend_from_slice(frame);
        let now = self.played.len() - FRAME_LEN;
        ::std::array::from_fn(|i| {
            let at = |delay: usize| (now + i).checked_sub(delay).map_or(0., |n| self.played[n]);
            0.3 * at(Self::DELAY) + 0.1 * at(Self::REFLECTION)
        })
    }
}

#[test]
fn removes_the_echo_of_the_reference() {
    let (mut echo, mut reference) = Echo::new();
    let (mut noise, mut room) = (Noise(0x2545_f491), Room { played: Vec::new() });
    let (mut energy_in, mut energy_out) = (0., 0.);
    // 5 s, the last second compared, after AEC3 converged
    for i in 0..500 {
        let played = noise.frame();
        let mut frame = room.echo(&played);
        reference.push(&played);
        let input = frame;
        echo.process(&mut frame).unwrap();
        if i >= 400 {
            energy_in += energy(&input);
            energy_out += energy(&frame);
        }
    }
    let removed_db = 10. * (energy_in / energy_out).log10();
    assert!(removed_db > 25., "{removed_db:.1} dB removed");
    // what it reports: it ran, took the reference, sees the echo removed and the room's delay
    let status = echo.status();
    assert_eq!(status.capture_frames(), 500);
    assert_eq!(status.reference_frames(), 500);
    assert!(status.erle_db() > 10., "ERLE {} dB", status.erle_db());
    let delay_ms = Room::DELAY as f32 * 1000. / ::voice_filter::SAMPLE_RATE as f32;
    assert!(
        (status.delay_ms() - delay_ms).abs() <= 4.,
        "delay {} ms, the room's {delay_ms} ms",
        status.delay_ms()
    );
}

#[test]
fn leaves_the_microphone_alone_without_a_reference() {
    // speech-like noise, no reference: it comes out delayed by the latency, at its level
    let (mut echo, _reference) = Echo::new();
    let mut noise = Noise(0x1234_5678);
    let (mut input, mut output) = (Vec::new(), Vec::new());
    for _ in 0..220 {
        let mut frame = noise.frame();
        input.extend_from_slice(&frame);
        echo.process(&mut frame).unwrap();
        output.extend_from_slice(&frame);
    }
    let start = 100 * FRAME_LEN;
    let correlation = |lag: usize| -> f64 {
        (start..start + 48000)
            .map(|i| input[i] as f64 * output[i + lag] as f64)
            .sum()
    };
    let best = (0..1000)
        .max_by(|&a, &b| correlation(a).total_cmp(&correlation(b)))
        .unwrap();
    assert_eq!(best, LATENCY);
    assert_eq!(echo.latency(), LATENCY);
    // its high-pass and the band split take a little
    let gain = correlation(LATENCY) / energy(&input[start..start + 48000]);
    assert!(gain > 0.85, "{gain:.3}");
}

#[test]
fn allocates_nothing_per_frame() {
    let (mut echo, mut reference) = Echo::new();
    let (mut noise, mut room) = (
        Noise(0x2545_f491),
        Room {
            played: Vec::with_capacity(1 << 20),
        },
    );
    let mut run = |echo: &mut Echo| {
        let played = noise.frame();
        let mut frame = room.echo(&played);
        reference.push(&played);
        echo.process(&mut frame).unwrap();
    };
    // the first frames set up what AEC3 keeps
    for _ in 0..100 {
        run(&mut echo);
    }
    let before = ALLOCATIONS.with(Cell::get);
    for _ in 0..500 {
        run(&mut echo);
    }
    let allocations = ALLOCATIONS.with(Cell::get) - before;
    assert_eq!(allocations, 0, "allocations in 500 frames");
}

#[test]
fn hands_on_the_microphone_delayed_by_its_latency() {
    let (mut echo, _reference) = Echo::new();
    let sidechain = echo.sidechain();
    let mut noise = Noise(0x2545_f491);
    let mut input = Vec::new();
    let mut raw = Vec::new();
    for frames in 1..=10 {
        let mut frame = noise.frame();
        input.extend_from_slice(&frame);
        echo.process(&mut frame).unwrap();
        assert_eq!(sidechain.frames(), frames);
        let mut out = [0.; FRAME_LEN];
        sidechain.raw(&mut out);
        raw.extend_from_slice(&out);
    }
    assert!(raw[..LATENCY].iter().all(|x| *x == 0.));
    assert_eq!(raw[LATENCY..], input[..input.len() - LATENCY]);
}

/// Runs `frames` frames of `played` through `echo`, picked up in a room; returns whether it
/// takes the speakers for playing after the last.
fn play(echo: &mut Echo, reference: &mut Reference, frames: usize, amplitude: f32) -> bool {
    let (mut noise, mut room) = (Noise(0x2545_f491), Room { played: Vec::new() });
    for _ in 0..frames {
        let played = noise.frame().map(|x| x * amplitude);
        let mut frame = room.echo(&played);
        reference.push(&played);
        echo.process(&mut frame).unwrap();
    }
    echo.status().playing()
}

#[test]
fn knows_whether_the_speakers_play() {
    let (mut echo, mut reference) = Echo::new();
    let sidechain = echo.sidechain();
    assert!(!echo.status().playing());
    assert!(play(&mut echo, &mut reference, 10, 1.));
    assert!(sidechain.playing());
    assert_eq!(echo.trace(), Some(("playing", 1.)));
    // a short pause keeps it, a second of silence ends it
    assert!(play(&mut echo, &mut reference, SILENCE_FRAMES - 1, 0.));
    assert!(!play(&mut echo, &mut reference, 1, 0.));
    assert!(play(&mut echo, &mut reference, 1, 1.));
    // the speakers' stream stops: no reference for a second
    let mut frame = [0.; FRAME_LEN];
    for _ in 0..SILENCE_FRAMES {
        echo.process(&mut frame).unwrap();
    }
    assert!(!echo.status().playing());
    assert_eq!(echo.trace(), Some(("playing", 0.)));
}

#[test]
fn drops_what_does_not_fit_into_the_reference() {
    let (_echo, mut reference) = Echo::new();
    let samples = vec![0.1; 10_000];
    assert_eq!(reference.push(&samples), 0);
    // the ring holds 16384 samples
    assert_eq!(reference.push(&samples), 10_000 - (16_384 - 10_000));
}
