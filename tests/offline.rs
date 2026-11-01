mod common;

use ::anyhow::Result;
use ::hound::{SampleFormat, WavSpec, WavWriter};
use ::voice_filter::chain::Chain;
use ::voice_filter::echo::Echo;
use ::voice_filter::offline;
use ::voice_filter::stage::{Frame, Stage};
use ::voice_filter::{FRAME_LEN, SAMPLE_RATE};
use common::Noise;

/// Delays the signal by whole samples, across frame boundaries.
struct Delay {
    buffer: Vec<f32>,
}

impl Delay {
    fn new(samples: usize) -> Self {
        Self {
            buffer: vec![0.; samples],
        }
    }
}

impl Stage for Delay {
    fn name(&self) -> &'static str {
        "delay"
    }

    fn latency(&self) -> usize {
        self.buffer.len()
    }

    fn process(&mut self, frame: &mut Frame) -> Result<()> {
        let mut all: Vec<f32> = self.buffer.iter().chain(frame.iter()).copied().collect();
        let tail = all.split_off(FRAME_LEN);
        frame.copy_from_slice(&all);
        self.buffer.copy_from_slice(&tail);
        Ok(())
    }
}

fn ramp(len: usize) -> Vec<f32> {
    (0..len).map(|i| i as f32 / len as f32).collect()
}

#[test]
fn process_lines_the_output_up_with_the_input() {
    // not a multiple of the frame length, and a delay over more than a frame
    let input = ramp(10 * FRAME_LEN + 123);
    let mut chain = Chain::new(vec![Box::new(Delay::new(FRAME_LEN + 17))]);
    let (output, report) = offline::process(&mut chain, &input, None).unwrap();
    assert_eq!(output, input);
    assert_eq!(report.latency(), FRAME_LEN + 17);
    assert_eq!(report.frames, 12);
    assert_eq!(report.stages[0].name, "delay");
}

#[test]
fn reads_int_stereo_as_mono_and_writes_float() {
    let dir = ::tempfile::tempdir().unwrap();
    let input = dir.path().join("in.wav");
    let spec = WavSpec {
        channels: 2,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };
    let mut writer = WavWriter::create(&input, spec).unwrap();
    for (left, right) in [(16384i16, 0i16), (-32768, -32768), (0, 8192)] {
        writer.write_sample(left).unwrap();
        writer.write_sample(right).unwrap();
    }
    writer.finalize().unwrap();
    assert_eq!(offline::read_wav(&input).unwrap(), [0.25, -1., 0.125]);

    let output = dir.path().join("out.wav");
    offline::write_wav(&output, &[0.5, -0.25]).unwrap();
    let reader = ::hound::WavReader::open(&output).unwrap();
    assert_eq!(reader.spec().channels, 1);
    assert_eq!(reader.spec().sample_format, SampleFormat::Float);
    assert_eq!(offline::read_wav(&output).unwrap(), [0.5, -0.25]);
    // the channel mask is "front center", players put mono on both speakers
    let header = ::std::fs::read(&output).unwrap();
    assert_eq!(header[40..44], 0x4u32.to_le_bytes());
}

#[test]
fn rejects_other_sample_rates() {
    let dir = ::tempfile::tempdir().unwrap();
    let path = dir.path().join("44100.wav");
    let spec = WavSpec {
        channels: 1,
        sample_rate: 44100,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };
    WavWriter::create(&path, spec).unwrap().finalize().unwrap();
    let err = offline::read_wav(&path).unwrap_err().to_string();
    assert!(err.contains("44100 Hz"), "{err}");
}

fn energy(samples: &[f32]) -> f64 {
    samples.iter().map(|&x| (x as f64).powi(2)).sum()
}

#[test]
fn removes_the_echo_of_the_reference_file() {
    let dir = ::tempfile::tempdir().unwrap();
    let (input, played, output) = (
        dir.path().join("in.wav"),
        dir.path().join("played.wav"),
        dir.path().join("out.wav"),
    );
    // 5 s of what the speakers played, the microphone picks it up 4 ms later at a third
    let len = 5 * SAMPLE_RATE as usize;
    let reference = Noise(0x2545_f491).samples(len);
    let echo: Vec<f32> = (0..len)
        .map(|i| i.checked_sub(192).map_or(0., |n| 0.3 * reference[n]))
        .collect();
    offline::write_wav(&played, &reference).unwrap();
    offline::write_wav(&input, &echo).unwrap();

    let (stage, mut feed) = Echo::new();
    let mut chain = Chain::new(vec![Box::new(stage)]);
    let report =
        offline::process_file(&mut chain, &input, &output, Some((&mut feed, &played))).unwrap();
    let processed = offline::read_wav(&output).unwrap();
    assert_eq!(processed.len(), len);
    // the last second, after AEC3 converged
    let last = len - SAMPLE_RATE as usize..;
    let removed_db = 10. * (energy(&echo[last.clone()]) / energy(&processed[last])).log10();
    assert!(removed_db > 25., "{removed_db:.1} dB removed");

    // the report has a row per stage and the total; one frame more for the latency of the echo
    // canceller, padded with silence
    let printed = report.to_string();
    assert!(
        printed.starts_with("5.0 s, 501 frames of 10 ms\n"),
        "{printed}"
    );
    for row in ["echo ", "total "] {
        assert!(
            printed.lines().any(|line| line.starts_with(row)),
            "{row:?} in {printed}"
        );
    }
    assert!(
        printed.ends_with("of the 10 ms a frame may take live"),
        "{printed}"
    );
}

#[test]
fn leaves_the_input_alone_without_a_reference() {
    let input = Noise(7).samples(50 * FRAME_LEN);
    let (stage, _feed) = Echo::new();
    let mut chain = Chain::new(vec![Box::new(stage)]);
    let (output, _) = offline::process(&mut chain, &input, None).unwrap();
    // AEC3 without a reference keeps the signal, up to rounding
    let changed_db = 10. * (energy(&output) / energy(&input)).log10();
    assert!(changed_db.abs() < 0.5, "{changed_db:.2} dB");
}
