use ::anyhow::Result;
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

/// Delays by `latency` samples and doubles.
struct DelayedGain {
    buffer: Vec<f32>,
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
        let tail = all.split_off(::voice_filter::FRAME_LEN);
        for (out, x) in frame.iter_mut().zip(&all) {
            *out = 2. * x;
        }
        self.buffer.copy_from_slice(&tail);
        Ok(())
    }
}

#[test]
fn switches_stages_without_changing_the_latency() {
    use ::std::sync::atomic::Ordering;
    use ::voice_filter::FRAME_LEN;

    let mut chain = Chain::new(vec![Box::new(DelayedGain {
        buffer: vec![0.; 100],
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

    let (first, frame) = run(&mut chain);
    assert_eq!(frame[100], 2. * first);

    // off: the crossfade frame lies between, then the input delayed by the same 100 samples
    enabled.store(false, Ordering::Relaxed);
    let (first, frame) = run(&mut chain);
    let delayed = first - 100. + 400.;
    assert!(frame[400] > delayed && frame[400] < 2. * delayed);
    let (first, frame) = run(&mut chain);
    for (i, &sample) in frame.iter().enumerate() {
        assert_eq!(sample, first - 100. + i as f32);
    }

    // on again: back to the stage's output after the crossfade frame
    enabled.store(true, Ordering::Relaxed);
    run(&mut chain);
    let (first, frame) = run(&mut chain);
    for (i, &sample) in frame.iter().enumerate() {
        assert_eq!(sample, 2. * (first - 100. + i as f32));
    }
    assert_eq!(chain.latency(), 100);
}
