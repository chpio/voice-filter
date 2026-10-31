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
