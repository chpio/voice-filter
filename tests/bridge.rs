use ::std::sync::atomic::Ordering;

use ::voice_filter::FRAME_LEN;
use ::voice_filter::bridge::bridge;
use ::voice_filter::chain::Chain;

/// Simulates the two callbacks with the same quantum, the capture side first or second in every
/// cycle. After the start no silence may be played and nothing dropped.
fn run(quantum: usize, capture_first: impl Fn(usize) -> bool) -> Vec<f32> {
    let (mut capture, mut playback, stats) = bridge(Chain::new(vec![]));
    // a counter as signal, so a gap or repetition shows up as a jump
    let mut next = 1.;
    let mut output = vec![];
    let mut buffer = vec![0.; quantum];
    let mut underruns_at_start = 0;
    for cycle in 0..2000 {
        let input: Vec<f32> = (0..quantum)
            .map(|_| {
                next += 1.;
                next
            })
            .collect();
        if capture_first(cycle) {
            capture.push(&input);
            playback.pull(&mut buffer);
        } else {
            playback.pull(&mut buffer);
            capture.push(&input);
        }
        output.extend_from_slice(&buffer);
        if cycle == 20 {
            underruns_at_start = stats.underruns.load(Ordering::Relaxed);
        }
    }
    assert_eq!(
        stats.underruns.load(Ordering::Relaxed),
        underruns_at_start,
        "underruns after the start, quantum {quantum}"
    );
    assert_eq!(
        stats.dropped.load(Ordering::Relaxed),
        0,
        "quantum {quantum}"
    );
    output
}

fn assert_continuous(output: &[f32], quantum: usize) {
    // skip the start, where the buffer settles and silence is played
    let settled = &output[40 * quantum.max(FRAME_LEN)..];
    for pair in settled.windows(2) {
        assert_eq!(pair[1], pair[0] + 1., "gap with quantum {quantum}");
    }
}

#[test]
fn keeps_up_with_any_quantum_and_order() {
    for quantum in [64, 128, 256, 480, 512, 960, 1024, 2048] {
        assert_continuous(&run(quantum, |_| true), quantum);
        assert_continuous(&run(quantum, |_| false), quantum);
        // the order changing from cycle to cycle
        assert_continuous(&run(quantum, |cycle| cycle % 3 != 0), quantum);
    }
}

#[test]
fn drops_what_piled_up_while_paused() {
    let (mut capture, mut playback, stats) = bridge(Chain::new(vec![]));
    let mut buffer = [0.; 256];
    // nobody pulls for a while: the ring fills up and then drops
    for _ in 0..100 {
        capture.push(&[1.; 256]);
    }
    assert!(stats.dropped.load(Ordering::Relaxed) > 0);
    // the first pull trims the level back to the quantum and a frame
    playback.pull(&mut buffer);
    assert_eq!(playback.buffered(), buffer.len() + FRAME_LEN);
}

#[test]
fn runs_the_chain_on_whole_frames() {
    let (mut capture, mut playback, stats) = bridge(Chain::new(vec![]));
    capture.push(&[0.5; FRAME_LEN - 1]);
    assert_eq!(stats.frames.load(Ordering::Relaxed), 0);
    assert_eq!(playback.buffered(), 0);
    capture.push(&[0.5; 2]);
    assert_eq!(stats.frames.load(Ordering::Relaxed), 1);
    assert_eq!(playback.buffered(), FRAME_LEN);
    let mut buffer = [0.; FRAME_LEN];
    playback.pull(&mut buffer);
    assert!(buffer.iter().all(|&sample| sample == 0.5));
}
