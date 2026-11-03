# Measurements

The numbers behind the defaults of the noise suppression and the order of the chain, with how they
were measured, so they can be repeated. The decisions themselves are in the [concept](concept.md).

**Contents:** [Method](#method) · [Skipping frames](#skipping-frames-by-the-estimated-snr) ·
[Low-latency variant](#low-latency-variant) · [High-pass and input level](#high-pass-and-input-level)
· [Zero mask](#zero-mask) · [Open](#open)

## Method

- **VoiceBank-DEMAND** (Valentini-Botinhao, 2017, CC BY 4.0, <https://doi.org/10.7488/ds/2117>):
  its test set, 824 sentences of two speakers, each clean and mixed with recorded noise (DEMAND),
  48 kHz; the speech sits at about −22 dBFS (median of the sentences, 10–90 %: −24 to −20). The
  only test with a clean reference. Every sentence runs through `DfTract` alone, with a fresh state
  (the sentences are about 2.5 s long, so the model's settling counts), the output lined up with
  the input. Scores against the clean sentence: PESQ wideband (ITU-T P.862.2, resampled to 16 kHz;
  1 to 4.5), STOI (intelligibility, 0 to 1) and SI-SDR (useless after an IIR high-pass, which
  shifts the phase against the reference), with the Python packages `pesq`, `pystoi`, `scipy` and
  `soundfile`.
- **Recordings** of a USB microphone (at 30–40 cm) and of a laptop's built-in one (next to its
  fans), raw: no clean reference, so measured by levels and checked by listening.
  `examples/trace.rs --interval 0.01` gives the level after every stage and the estimates per
  frame. A frame of the input is a **pause** where the median level within ±150 ms is less than
  8 dB above the noise floor (the quietest frame within ±1 s), **speech** where it's more than
  20 dB above it. **Noise in a pause:** an output frame above −60 dBFS. **Speech lost:** a speech
  frame that comes out below −45 dBFS while the chain without the high-pass in front (echo
  cancellation off) keeps it above −30 dBFS.

**Repeating them:** `examples/batch.rs` runs every file of a directory through the chain or some of
its stages (`--only`, `--off`, `--set`, `--thresholds` for the skipping), each with a fresh state,
`scripts/score.py` scores the outputs against the clean files, `examples/trace.rs` traces a
recording per frame and `scripts/trace_stats.py` takes the pauses, the noise in them and the lost
speech from traces:

```bash
# the Python packages, once (or `uv run scripts/score.py …`, which reads them from the script)
python3 -m venv .venv && .venv/bin/pip install numpy scipy soundfile pesq pystoi
# VoiceBank-DEMAND: two variants of the noise suppression alone, then their scores
cargo run --release --example batch -- noisy_testset_wav/ out/default/ --only denoise
cargo run --release --example batch -- noisy_testset_wav/ out/upstream/ --only denoise \
  --thresholds -10,30,20
.venv/bin/python scripts/score.py clean_testset_wav/ noisy_testset_wav/ out/default/ out/upstream/
# a recording: a trace of each variant, the first one is what the others are measured against
cargo run --release --example trace -- call.wav --off echo --interval 0.01 > now.tsv
cargo run --release --example trace -- call.wav --off echo --interval 0.01 \
  --set highpass.freq=60 > hp60.tsv
.venv/bin/python scripts/trace_stats.py now.tsv hp60.tsv --events 5
```

## Skipping frames by the estimated SNR

Upstream's real-time defaults skip parts of the model by its estimate of the local SNR: zeros
instead of its gains below −10 dB (`min_db_thresh`), the frame unchanged above 30 dB
(`max_db_erb_thresh`), no deep filter above 20 dB (`max_db_df_thresh`). VoiceBank-DEMAND, the noise
suppression alone, unlimited attenuation:

| DeepFilterNet3                                | PESQ |  STOI |  SI-SDR | per frame |
| --------------------------------------------- | ---: | ----: | ------: | --------: |
| the noisy input                               | 1.97 | 0.921 |  8.4 dB |           |
| upstream's defaults (−10, 30, 20 dB)          | 2.07 | 0.861 |  9.0 dB |   0.27 ms |
| only the zeros below −10 dB (the default now) | 2.70 | 0.925 | 18.7 dB |   0.31 ms |
| only the skipping above 30 and 20 dB          | 2.48 | 0.881 |  9.3 dB |           |
| nothing skipped                               | 3.16 | 0.945 | 18.9 dB |   0.37 ms |

Nothing skipped matches upstream's published PESQ (3.17). With upstream's defaults the
intelligibility fell below the noisy input's. Measured from 1 s into every sentence the gap stays
(2.29 against 2.67 for the low-latency variant below with the same thresholds), so it isn't the
model settling.

## Low-latency variant

Upstream's `DeepFilterNet3_ll` has no lookahead (`conv_lookahead` and `df_lookahead` 0 instead of
2) and makes up for it with a larger network: GRUs of 512 instead of 256 units, three layers in the
deep filter's decoder instead of two, convolutions over two frames instead of one; 39 MB of ONNX
instead of 8.6 MB. VoiceBank-DEMAND, nothing skipped:

|                | delay | per frame | PESQ |  STOI |  SI-SDR |
| -------------- | ----: | --------: | ---: | ----: | ------: |
| DeepFilterNet3 | 30 ms |   0.37 ms | 3.16 | 0.945 | 18.9 dB |
| the LL variant | 10 ms |    1.8 ms | 3.07 | 0.950 | 18.7 dB |

With upstream's skipping it scored 2.49 against 2.07 (1.19 against 0.27 ms per frame): it suffers
less from the skipping, not a better model. Not used (see the decisions in the concept).

## High-pass and input level

A call over headphones (USB microphone, the echo cancellation off as there's no echo, 10 minutes,
33 % pauses, 16 % speech), the chain after the skipping fix:

| Chain                                                | noise in pauses | estimate > 0 dB in pauses | speech out | speech lost |
| ---------------------------------------------------- | --------------: | ------------------------: | ---------: | ----------: |
| high-pass after the noise suppression (before)       |          54.5 % |                    43.1 % | −19.0 dBFS |           – |
| the same with the echo cancellation on               |           3.5 % |                     3.4 % | −22.6 dBFS |      13.2 % |
| high-pass (80 Hz) first                              |           6.4 % |                     5.6 % | −22.6 dBFS |      12.2 % |
| high-pass first, 10 dB more into the model           |          11.8 % |                     1.1 % | −18.9 dBFS |       2.7 % |
| high-pass first, without the zero mask               |           7.0 % |                     5.6 % | −22.7 dBFS |      10.7 % |
| high-pass first, 10 dB more, without the zero mask   |          14.2 % |                     1.1 % | −18.9 dBFS |       2.2 % |
| high-pass first at 40 Hz, with the input gain        |          53.8 % |                    15.5 % | −19.0 dBFS |       0.6 % |
| high-pass first, with the input gain (the chain now) |           5.6 % |                     3.4 % | −19.1 dBFS |       2.3 % |
| the same with the echo cancellation on               |           4.0 % |                     2.7 % | −19.1 dBFS |       2.6 % |

- The microphone's noise lies mostly below 100 Hz (in a quiet second: 20–50 Hz 35 % of the energy,
  below 20 Hz 10 %, 50–100 Hz 11 %). AEC3 filters it out (sonora forces WebRTC's high-pass with
  it, `EchoCanceller::enforce_high_pass_filtering`), so the echo cancellation hid the problem;
  off, DeepFilterNet3 took the rumble for speech, the AGC raised it by about 23 dB and the
  expander opened.
- Without the rumble the model hears the voice quieter too, and it judges by the level: with the
  high-pass first alone, or the echo cancellation on, it muted speech at −39 and −33 dBFS (checked
  by listening), which 10 dB more into the model kept. A noise there (something moved, −31 dBFS)
  the chain before let through at −20 dBFS, every chain with the high-pass first muted it.
- The input gain (`denoise` `level` −35 dBFS, `max-gain` 20 dB) on the other recordings, noise in
  pauses before and now: the built-in microphone along on the same call 58.3 → 1.5 %, the built-in
  one in a minute of silence 5.0 → 4.8 %, the USB one there 2.7 → 3.3 %, the USB one with a video
  over the laptop's speakers 3.0 → 2.7 % (its speech out −22.3 → −21.7 dBFS); the built-in one far
  from the speaker (speech at −58 dBFS, under the echo) stayed muted.
- VoiceBank-DEMAND, the high-pass first against after the noise suppression (both filtered, so
  the reference compares the same; the input gain doesn't act at −22 dBFS):

  | DeepFilterNet3, zeros below −10 dB | PESQ |  STOI | PESQ by input SNR: < 5, 5–10, 10–15, > 15 dB |
  | ---------------------------------- | ---: | ----: | -------------------------------------------- |
  | high-pass (80 Hz) after            | 2.70 | 0.924 | 1.81, 2.47, 3.11, 3.53                       |
  | high-pass (80 Hz) first            | 2.65 | 0.917 | 2.15, 2.56, 2.82, 3.13                       |
  | high-pass at 40 Hz first           | 2.75 | 0.923 |                                              |

  Without the zero mask: 3.14 after, 2.91 first (2.63, 3.08, 3.33, 3.59 against 2.48, 2.89, 3.05,
  3.29).
- Heard live with the chain now (2026-11-01): much less noise in the pauses, on the built-in
  microphone too.

## Zero mask

Whether the zeros below −10 dB stay is open. Through the whole chain (`voice-filter process`, so
with the echo cancellation on, before the high-pass moved first), noise in pauses with the zeros
and without:

| Recording                                | median, with | noise, with | median, without | noise, without |
| ---------------------------------------- | -----------: | ----------: | --------------: | -------------: |
| the call, USB microphone                 |    −120 dBFS |       3.5 % |        −97 dBFS |          4.0 % |
| the call, built-in microphone            |    −120 dBFS |       1.5 % |        −50 dBFS |         72.8 % |
| a minute of silence, USB microphone      |     −80 dBFS |       2.6 % |        −77 dBFS |          3.5 % |
| a minute of silence, built-in microphone |    −120 dBFS |       4.5 % |        −44 dBFS |         75.6 % |

Without the zeros the fan noise of the built-in microphone came through, the expander's level
criterion (−50 dBFS) kept it open; with them 17.8 % of its speech frames were muted, without 6.7 %.
On the USB microphone the zeros hardly matter.

## Denormals

In a pause `denoise` puts out exact zeros. The EQ's memory decays through the denormals of f32
after it (on the call with the USB microphone 517,671 denormal samples in 1,853 frames, about two
frames per pause, passed on down the chain), and the one-pole detectors of the de-esser and the
compressor stick at a denormal (`y * factor` rounds to nothing). `cargo bench --bench stages` on that
recording (61,248 frames, an AMD Ryzen 7 8840U), mean per frame, two runs each:

| Stage      | without flushing | denormals flushed |
| ---------- | ---------------: | ----------------: |
| eq         |    2.58, 2.62 µs |     1.30, 1.31 µs |
| deesser    |  12.02, 11.95 µs |     7.95, 8.05 µs |
| compressor |    4.78, 4.78 µs |     3.15, 3.18 µs |
| total      |  202.7, 199.2 µs |   192.8, 195.1 µs |

The other stages and the maxima per frame (1.2–1.9 ms, from `denoise`) stayed the same; fat LTO
with one codegen unit changed nothing beyond the noise either (202 and 196 µs). Intel CPUs take
longer over a denormal than this Zen 4, so it may save more there. Hence `dsp::without_denormals`
(flush to zero and denormals are zero, the `no_denormals` crate) around every frame of the chain.

## Open

- The zero mask: measure again with the high-pass first and the input gain, and listen.
- A high-pass between 40 and 80 Hz in front, which may cost less on VoiceBank-DEMAND and still keep
  the rumble from the model.
