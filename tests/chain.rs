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
