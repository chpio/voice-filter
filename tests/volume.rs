use ::voice_filter::FRAME_LEN;
use ::voice_filter::stage::Stage;
use ::voice_filter::volume::{self, Volume};

#[test]
fn turns_down_by_its_volume_without_steps() {
    let param = volume::param();
    let mut volume = Volume::new(param.clone());
    assert_eq!(volume.latency(), 0);
    let mut frame = [1.; FRAME_LEN];
    volume.process(&mut frame).unwrap();
    assert!(frame.iter().all(|&x| x == 1.));

    // -20 dB: a ramp over the next frame, then a tenth
    param.set(-20.).unwrap();
    let mut frame = [1.; FRAME_LEN];
    volume.process(&mut frame).unwrap();
    assert!(frame.windows(2).all(|pair| pair[1] <= pair[0]));
    assert!((frame[FRAME_LEN - 1] - 0.1).abs() < 1e-6);
    let mut frame = [1.; FRAME_LEN];
    volume.process(&mut frame).unwrap();
    assert!(frame.iter().all(|&x| (x - 0.1).abs() < 1e-6));

    // a new one takes the volume as it is
    let mut frame = [1.; FRAME_LEN];
    Volume::new(param).process(&mut frame).unwrap();
    assert!((frame[0] - 0.1).abs() < 1e-6);
}
