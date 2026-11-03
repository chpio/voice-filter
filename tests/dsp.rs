use ::std::hint::black_box;
use ::std::num::FpCategory;

use ::voice_filter::dsp::without_denormals;

/// Half the smallest normal f32, a denormal unless they're flushed.
fn half_of_smallest() -> f32 {
    black_box(f32::MIN_POSITIVE) * black_box(0.5)
}

#[test]
fn without_denormals_flushes_them_and_restores_the_flags() {
    assert_eq!(half_of_smallest().classify(), FpCategory::Subnormal);
    let inside = without_denormals(half_of_smallest);
    if cfg!(any(
        target_arch = "x86",
        target_arch = "x86_64",
        target_arch = "aarch64"
    )) {
        assert_eq!(inside, 0.);
    }
    assert_eq!(half_of_smallest().classify(), FpCategory::Subnormal);
}

#[test]
fn without_denormals_restores_the_flags_after_a_panic() {
    let result = ::std::panic::catch_unwind(|| without_denormals(|| panic!("in the closure")));
    assert!(result.is_err());
    assert_eq!(half_of_smallest().classify(), FpCategory::Subnormal);
}
