use ::voice_filter::FRAME_LEN;
use ::voice_filter::level::{self, Meter, NoiseFloor};

#[test]
fn measures_the_rms_level_of_a_frame() {
    assert_eq!(level::frame_db(&[0.; FRAME_LEN]), -120.);
    assert!((level::frame_db(&[0.1; FRAME_LEN]) - -20.).abs() < 0.01);
}

#[test]
fn takes_the_quietest_frame_of_the_last_two_seconds() {
    let mut floor = NoiseFloor::new();
    assert_eq!(floor.push(-30.), -30.);
    assert_eq!(floor.push(-50.), -50.);
    // louder frames keep the quiet one for 2 s (200 frames), then the floor follows them
    let floors: Vec<f32> = (0..200).map(|_| floor.push(-30.)).collect();
    assert!(floors[..180].iter().all(|&db| db == -50.), "{floors:?}");
    assert_eq!(floors[199], -30.);
}

#[test]
fn falls_by_20_db_per_second() {
    let mut meter = Meter::new();
    assert_eq!(meter.push(-120.), -120.);
    assert_eq!(meter.push(-10.), -10.);
    let mut db = 0.;
    for _ in 0..50 {
        db = meter.push(-120.);
    }
    assert!((db - -20.).abs() < 0.01, "{db}");
    // a louder frame at once
    assert_eq!(meter.push(-6.), -6.);
}
