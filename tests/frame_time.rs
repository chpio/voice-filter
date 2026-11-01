use ::voice_filter::frame_time::{FrameTime, Summary, WINDOW_SECONDS, Window};

#[test]
fn summarizes_the_frames_of_a_push() {
    let time = FrameTime::default();
    let mut window = Window::default();
    // 1000 frames of 1 to 1000 µs
    for us in 1..=1000 {
        time.record(us * 1000);
    }
    let summary = window.push(&time);
    assert_eq!(summary.frames, 1000);
    assert_eq!(summary.mean_ns, 500_500);
    assert_eq!(summary.max_ns, 1_000_000);
    // rounded up by at most 1/64
    for (percentile, exact) in [(summary.p95_ns, 950_000), (summary.p99_ns, 990_000)] {
        assert!(
            (exact..=exact + exact / 64).contains(&percentile),
            "{percentile} for {exact}"
        );
    }
    assert_eq!(time.last(), summary);
}

#[test]
fn the_window_keeps_the_last_seconds() {
    let time = FrameTime::default();
    let mut window = Window::default();
    assert_eq!(window.push(&time), Summary::default());
    // a slow frame, then seconds of 100 fast ones
    time.record(5_000_000);
    window.push(&time);
    for second in 1..WINDOW_SECONDS {
        for _ in 0..100 {
            time.record(200_000);
        }
        let summary = window.push(&time);
        assert_eq!(summary.frames, 1 + 100 * second as u64, "{summary:?}");
        assert_eq!(summary.max_ns, 5_000_000, "{summary:?}");
    }
    // the slow one leaves the window
    for _ in 0..100 {
        time.record(200_000);
    }
    let summary = window.push(&time);
    assert_eq!(
        summary,
        Summary {
            frames: 100 * WINDOW_SECONDS as u64,
            mean_ns: 200_000,
            p95_ns: 200_000,
            p99_ns: 200_000,
            max_ns: 200_000,
        }
    );
    assert_eq!(time.last(), summary);
    // and the others after it, one second each
    for _ in 0..WINDOW_SECONDS {
        window.push(&time);
    }
    assert_eq!(window.push(&time), Summary::default());
}

#[test]
fn the_percentiles_never_exceed_the_longest() {
    let time = FrameTime::default();
    for ns in [
        0,
        7,
        127,
        128,
        129,
        1000,
        1_000_001,
        2_000_000_000,
        10_000_000_000,
    ] {
        time.record(ns);
        let summary = Window::default().push(&time);
        assert!(summary.p99_ns <= summary.max_ns, "{summary:?}");
        assert!(summary.p99_ns >= ns - ns / 64, "{summary:?}");
    }
}

#[test]
fn the_rare_slow_frames_show_in_the_99th_percentile_only() {
    let time = FrameTime::default();
    let mut window = Window::default();
    // 2 % of the frames take ten times as long, over the seconds of the window
    for second in 0..WINDOW_SECONDS {
        for frame in 0..100 {
            let slow = (second * 100 + frame) % 50 == 0;
            time.record(if slow { 2_000_000 } else { 200_000 });
        }
        window.push(&time);
    }
    let summary = time.last();
    assert_eq!(summary.frames, 1000);
    assert!((200_000..=203_125).contains(&summary.p95_ns), "{summary:?}");
    assert!(
        (2_000_000..=2_031_250).contains(&summary.p99_ns),
        "{summary:?}"
    );
}
