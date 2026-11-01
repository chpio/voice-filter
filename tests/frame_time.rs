use ::voice_filter::frame_time::{FrameTime, Summary};

#[test]
fn summarizes_an_interval_and_starts_the_next() {
    let time = FrameTime::default();
    // 1000 frames of 1 to 1000 µs
    for us in 1..=1000 {
        time.record(us * 1000);
    }
    let summary = time.publish();
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

    // the next interval starts empty
    time.record(5000);
    let next = time.publish();
    assert_eq!(
        next,
        Summary {
            frames: 1,
            mean_ns: 5000,
            p95_ns: 5000,
            p99_ns: 5000,
            max_ns: 5000,
        }
    );
    assert_eq!(time.publish(), Summary::default());
}

#[test]
fn the_percentiles_never_exceed_the_longest() {
    let time = FrameTime::default();
    for ns in [0, 7, 127, 128, 129, 1000, 1_000_001, 2_000_000_000, 10_000_000_000] {
        time.record(ns);
        let summary = time.publish();
        assert!(summary.p99_ns <= summary.max_ns, "{summary:?}");
        assert!(summary.p99_ns >= ns - ns / 64, "{summary:?}");
    }
}

#[test]
fn the_rare_slow_frames_show_in_the_99th_percentile_only() {
    let time = FrameTime::default();
    // 2 % of the frames take ten times as long
    for frame in 0..1000 {
        time.record(if frame % 50 == 0 { 2_000_000 } else { 200_000 });
    }
    let summary = time.publish();
    assert!((200_000..=203_125).contains(&summary.p95_ns), "{summary:?}");
    assert!(
        (2_000_000..=2_031_250).contains(&summary.p99_ns),
        "{summary:?}"
    );
}
