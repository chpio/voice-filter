use ::std::sync::Arc;
use ::std::sync::atomic::{AtomicUsize, Ordering};

use ::anyhow::Result;
use ::voice_filter::FRAME_LEN;
use ::voice_filter::chain::Chain;
use ::voice_filter::stage::{Frame, Stage};

/// Multiplies by a factor and claims a latency, to see the order and the sum.
struct Gain {
    factor: f32,
    latency: usize,
}

impl Stage for Gain {
    fn name(&self) -> &'static str {
        "gain"
    }

    fn latency(&self) -> usize {
        self.latency
    }

    fn process(&mut self, frame: &mut Frame) -> Result<()> {
        frame.iter_mut().for_each(|sample| *sample *= self.factor);
        Ok(())
    }
}

#[test]
fn runs_the_stages_in_order_and_sums_their_latency() {
    let mut chain = Chain::new(vec![
        Box::new(Gain {
            factor: 2.,
            latency: 10,
        }),
        Box::new(Gain {
            factor: 0.25,
            latency: 32,
        }),
    ]);
    assert_eq!(chain.latency(), 42);

    let mut frame = [1.; _];
    chain.process(&mut frame).unwrap();
    assert!(frame.iter().all(|&sample| sample == 0.5));
}

/// Delays by `latency` samples and doubles; counts the frames it processed.
struct DelayedGain {
    buffer: Vec<f32>,
    processed: Arc<AtomicUsize>,
}

impl Stage for DelayedGain {
    fn name(&self) -> &'static str {
        "delayed-gain"
    }

    fn latency(&self) -> usize {
        self.buffer.len()
    }

    fn process(&mut self, frame: &mut Frame) -> Result<()> {
        let mut all: Vec<f32> = self.buffer.iter().chain(frame.iter()).copied().collect();
        let tail = all.split_off(FRAME_LEN);
        for (out, x) in frame.iter_mut().zip(&all) {
            *out = 2. * x;
        }
        self.buffer.copy_from_slice(&tail);
        self.processed.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
}

#[test]
fn switches_stages_without_changing_the_latency() {
    let processed = Arc::new(AtomicUsize::new(0));
    let mut chain = Chain::new(vec![Box::new(DelayedGain {
        buffer: vec![0.; 100],
        processed: processed.clone(),
    })]);
    let switches = chain.switches();
    assert_eq!(switches.len(), 1);
    assert_eq!(switches[0].0, "delayed-gain");
    let enabled = switches[0].1.clone();

    // input sample n has the value n + 1, so the output tells which sample it is
    let mut next = 0.;
    let mut run = |chain: &mut Chain| {
        let mut frame = [0.; FRAME_LEN];
        for sample in &mut frame {
            next += 1.;
            *sample = next;
        }
        let first = frame[0];
        chain.process(&mut frame).unwrap();
        (first, frame)
    };
    let delayed = |first: f32, frame: &Frame| {
        for (i, &sample) in frame.iter().enumerate() {
            assert_eq!(sample, first - 100. + i as f32);
        }
    };
    let doubled = |first: f32, frame: &Frame| {
        for (i, &sample) in frame.iter().enumerate() {
            assert_eq!(sample, 2. * (first - 100. + i as f32));
        }
    };

    let (first, frame) = run(&mut chain);
    assert_eq!(frame[100], 2. * first);

    // off: the crossfade frame lies between, then the input delayed by the same 100 samples, and
    // the stage stops processing
    enabled.store(false, Ordering::Relaxed);
    let (first, frame) = run(&mut chain);
    let input = first - 100. + 400.;
    assert!(frame[400] > input && frame[400] < 2. * input);
    assert_eq!(processed.load(Ordering::Relaxed), 2);
    for _ in 0..3 {
        let (first, frame) = run(&mut chain);
        delayed(first, &frame);
    }
    assert_eq!(processed.load(Ordering::Relaxed), 2);

    // on again: a frame (resume_frames, the 100 samples rounded up) it runs without being played,
    // as it still holds samples from before the pause, then the crossfade frame, then its output
    enabled.store(true, Ordering::Relaxed);
    let (first, frame) = run(&mut chain);
    delayed(first, &frame);
    assert_eq!(processed.load(Ordering::Relaxed), 3);
    run(&mut chain);
    let (first, frame) = run(&mut chain);
    doubled(first, &frame);
    assert_eq!(chain.latency(), 100);
}

#[test]
fn switching_on_during_the_resume_frames_starts_them_again() {
    let mut chain = Chain::new(vec![Box::new(DelayedGain {
        buffer: vec![0.; 1000],
        processed: Arc::new(AtomicUsize::new(0)),
    })]);
    let enabled = chain.switches()[0].1.clone();
    // a constant input, so the delayed one is the same
    let run = |chain: &mut Chain| {
        let mut frame = [1.; FRAME_LEN];
        chain.process(&mut frame).unwrap();
        frame
    };
    run(&mut chain);
    enabled.store(false, Ordering::Relaxed);
    // the crossfade frame and the pause
    run(&mut chain);
    run(&mut chain);
    // 1000 samples: three frames before it's played again; off and on within them restarts them
    enabled.store(true, Ordering::Relaxed);
    run(&mut chain);
    enabled.store(false, Ordering::Relaxed);
    run(&mut chain);
    enabled.store(true, Ordering::Relaxed);
    for _ in 0..3 {
        assert!(run(&mut chain).iter().all(|&sample| sample == 1.));
    }
    // the crossfade frame, then doubled
    run(&mut chain);
    assert!(run(&mut chain).iter().all(|&sample| sample == 2.));
}

#[test]
fn the_stages_trace_what_they_applied() -> Result<()> {
    let (mut chain, _reference) = ::voice_filter::chain::build(None)?;
    // a loud tone, so the limiter acts
    for i in 0..20 {
        let mut frame: Frame = ::std::array::from_fn(|n| {
            let t = (i * FRAME_LEN + n) as f32 / ::voice_filter::SAMPLE_RATE as f32;
            0.9 * (::std::f32::consts::TAU * 1000. * t).sin()
        });
        for stage in chain.stages_mut() {
            stage.process(&mut frame)?;
        }
    }
    let traces: Vec<_> = chain
        .stages()
        .map(|stage| (stage.name(), stage.trace().map(|(name, _)| name)))
        .collect();
    assert_eq!(
        traces,
        [
            ("highpass", None),
            ("echo", Some("playing")),
            ("denoise", Some("snr")),
            ("eq", None),
            ("deesser", Some("reduction")),
            ("agc", Some("gain")),
            ("compressor", Some("reduction")),
            ("expander", Some("gain")),
            ("limiter", Some("gain")),
        ]
    );
    for stage in chain.stages() {
        if let Some((name, value)) = stage.trace() {
            assert!(value.is_finite(), "{}.{name} is {value}", stage.name());
            if name != "snr" && stage.name() != "agc" {
                // the reductions and gains that only turn down
                assert!(value <= 0., "{}.{name} is {value}", stage.name());
            }
        }
    }
    Ok(())
}

/// Takes `ms` per frame.
struct Slow {
    name: &'static str,
    ms: u64,
}

impl Stage for Slow {
    fn name(&self) -> &'static str {
        self.name
    }

    fn latency(&self) -> usize {
        0
    }

    fn process(&mut self, _: &mut Frame) -> Result<()> {
        ::std::thread::sleep(::std::time::Duration::from_millis(self.ms));
        Ok(())
    }
}

#[test]
fn times_the_stages_asked_for() {
    let mut chain = Chain::new(vec![
        Box::new(Slow {
            name: "first",
            ms: 2,
        }),
        Box::new(Slow {
            name: "second",
            ms: 4,
        }),
        Box::new(Slow {
            name: "third",
            ms: 1,
        }),
    ]);
    assert!(chain.times().is_empty());
    chain.time(&["second", "first", "missing"]);
    assert_eq!(chain.times(), [0, 0, 0]);
    chain.process(&mut [0.; FRAME_LEN]).unwrap();
    let ms = |ns: u64| ns as f64 / 1e6;
    let [second, first, missing] = chain.times() else {
        panic!("{:?}", chain.times());
    };
    // sleeping takes at least as long, a busy machine may add to it
    assert!((2. ..20.).contains(&ms(*first)), "{first}");
    assert!((4. ..20.).contains(&ms(*second)), "{second}");
    assert_eq!(*missing, 0);
}
