//! Long digital silence with denormals flushed to zero, as real-time hosts run it (upstream issue
//! #713: NaN after 78 s).

#![cfg(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64"))]

use ::deep_filter_rt::Complex32;
use ::deep_filter_rt::tract::DfTract;
use ::ndarray::Array2;

/// 80 s in frames of 10 ms, past the 7,774 frames the unit norm state of the highest bins took
/// from its initial 1e-4 to below the smallest normal f32.
const FRAMES: usize = 8000;

fn flushed<T>(f: impl FnOnce() -> T) -> T {
    // SAFETY: the code inside only computes on floats, denormals flushed to zero change results
    // below 1.2e-38 only; the guard restores the flags
    unsafe { ::no_denormals::no_denormals(f) }
}

#[test]
fn the_unit_norm_stays_finite() {
    let df = DfTract::default();
    let (mut a, mut b) = (df.df_states[0].clone(), df.df_states[0].clone());
    let silence = vec![Complex32::new(0., 0.); df.nb_df];
    let mut cplx = vec![Complex32::new(1., 1.); df.nb_df];
    let mut cplx_t = vec![1.; 2 * df.nb_df];
    flushed(|| {
        for _ in 0..FRAMES {
            a.feat_cplx(&silence, df.alpha, &mut cplx);
            b.feat_cplx_t(&silence, df.alpha, &mut cplx_t);
        }
    });
    assert!(cplx.iter().all(|x| x.re == 0. && x.im == 0.), "{cplx:?}");
    assert!(cplx_t.iter().all(|&x| x == 0.), "{cplx_t:?}");
}

#[test]
fn processes_long_silence_and_sound_after_it() {
    let mut df = DfTract::default();
    let mut enh = Array2::zeros((1, df.hop_size));
    let silence = Array2::zeros((1, df.hop_size));
    flushed(|| {
        for _ in 0..FRAMES {
            let lsnr = df.process(silence.view(), enh.view_mut()).unwrap();
            assert!(lsnr.is_finite());
            assert!(enh.iter().all(|x| x.is_finite()));
        }
        // a tone after it: the output is finite and not silent
        let mut energy = 0.;
        for i in 0..100 {
            let tone = Array2::from_shape_fn((1, df.hop_size), |(_, n)| {
                let t = (i * 480 + n) as f32 / 48000.;
                0.1 * (::std::f32::consts::TAU * 440. * t).sin()
            });
            let lsnr = df.process(tone.view(), enh.view_mut()).unwrap();
            assert!(lsnr.is_finite());
            assert!(enh.iter().all(|x| x.is_finite()));
            energy += enh.iter().map(|x| x * x).sum::<f32>();
        }
        assert!(energy > 0.);
    });
}
