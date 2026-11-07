#![cfg(feature = "gui")]

use ::voice_filter::gui::proxy::Parameter;

fn parameter(min: f64, max: f64, step: f64) -> Parameter {
    Parameter {
        stage: "highpass".to_owned(),
        name: "freq".to_owned(),
        title: "Frequency".to_owned(),
        unit: "Hz".to_owned(),
        min,
        max,
        step,
        default: min,
        value: min,
    }
}

/// A dragged slider gives any value, the app sends it on a step.
#[test]
fn snaps_to_the_step() {
    let freq = parameter(20., 300., 5.);
    assert_eq!(freq.snap(83.71234), 85.);
    assert_eq!(freq.snap(82.4), 80.);
    assert_eq!(freq.snap(80.), 80.);
    // from the minimum, not from 0
    let level = parameter(-42., 0., 4.);
    assert_eq!(level.snap(-39.), -38.);
}

#[test]
fn stays_within_the_range() {
    let freq = parameter(20., 300., 5.);
    assert_eq!(freq.snap(-10.), 20.);
    assert_eq!(freq.snap(400.), 300.);
    // a maximum off the steps gives the last step below it
    let odd = parameter(0., 10., 3.);
    assert_eq!(odd.snap(10.), 9.);
    assert_eq!(odd.snap(11.), 10.);
}

#[test]
fn snaps_to_a_fraction() {
    let ratio = parameter(1., 10., 0.1);
    assert!((ratio.snap(3.04999) - 3.).abs() < 1e-9);
    assert!((ratio.snap(3.05001) - 3.1).abs() < 1e-9);
}

#[test]
fn a_step_of_zero_only_clamps() {
    let free = parameter(0., 1., 0.);
    assert_eq!(free.snap(0.123), 0.123);
    assert_eq!(free.snap(2.), 1.);
}
