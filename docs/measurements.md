# Measurements

The numbers behind the defaults of the noise suppression and the order of the chain, with how they
were measured, so they can be repeated. The decisions themselves are in the [concept](concept.md).

**Contents:** [Method](#method) · [Skipping frames](#skipping-frames-by-the-estimated-snr) ·
[Low-latency variant](#low-latency-variant) · [High-pass and input level](#high-pass-and-input-level)
· [Zero mask](#zero-mask) · [Steady tones](#steady-tones) · [Quantum](#quantum) ·
[AEC3 tuning](#aec3-tuning-in-double-talk) · [Open](#open)

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
recording per frame (`--thresholds` too) and `scripts/trace_stats.py` takes the pauses, the noise in
them and the lost speech from traces (`--speech-above 12` for a noisy microphone, whose speech lies
less than 20 dB above its noise floor); `scripts/tones.py` finds steady tones in a recording and
writes a copy without them (`--remove`):

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

Again with the chain as it is now (2026-11-04: the high-pass first, `denoise` raising its input,
echo on without a reference), `trace --interval 0.01`, the zeros dropped with `--thresholds
-100,1000,1000`, `trace_stats.py --speech-above 12` against the chain without the zeros; *speech
quiet* are the speech frames that come out below −45 dBFS (which *speech lost* only counts where
the reference keeps them above −30 dBFS, never here):

| Built-in microphone                         | noise in pauses | median | speech lost | speech quiet |
| ------------------------------------------- | --------------: | -----: | ----------: | -----------: |
| fans, a minute of silence: with the zeros   |           4.9 % |  −120 |       0.0 % |       18.4 % |
| without them                                |          74.4 % |  −44.9 |       0.0 % |        9.6 % |
| without them, expander `level` −45 dBFS     |          66.0 % |  −44.9 |       0.0 % |        9.6 % |
| without them, expander `level` −40 dBFS     |           8.9 % |  −64.7 |       0.0 % |       18.0 % |
| the call (20 min): with the zeros           |           1.4 % |  −120 |       0.9 % |        8.0 % |
| without them                                |          72.1 % |  −50.1 |       0.0 % |        4.3 % |
| without them, expander `level` −45 dBFS     |          47.1 % |  −66.0 |       0.0 % |        4.4 % |
| without them, expander `level` −40 dBFS     |          18.7 % |  −69.0 |       0.0 % |        4.8 % |

Without the zeros the model lets broadband noise through, turned down, which the AGC and the
compressor raise again and the expander's level criterion keeps open (the estimate stays at −13
to −14 dB in the pauses, see [Steady tones](#steady-tones)). With the expander opening only above
−40 dBFS the pauses of the fans go about as quiet as with the zeros, but as much speech comes out
quiet; on the call it keeps the speech of the chain without the zeros and leaves 18.7 % of the
pause frames above −60 dBFS. Not listened to yet.

The zeros also save CPU: where the estimate is below −10 dB, `DfTract` runs only the encoder, not
the two decoders. `cargo bench --bench stages -- in.wav [--thresholds -100,1000,1000]` (2026-11-04,
the `x86-64-v4` build of this machine, see [Target CPU](#target-cpu); an idle AMD Ryzen 7 8840U,
three rounds, each with the zeros and without one after the other, the median of the three means
per frame, each the best of the benchmark's 5 passes), in µs:

| Recording                                | estimate < −10 dB | `denoise`, zeros | without | chain, zeros | without |
| ---------------------------------------- | ----------------: | ---------------: | ------: | -----------: | ------: |
| the call, USB microphone (61,248 frames) |            73.7 % |            137.1 |   252.5 |        156.3 |   272.5 |
| fans, built-in (10,244 frames)           |            75.6 % |            133.5 |   249.7 |        152.2 |   271.4 |

A frame with the zeros takes about 96 µs (the encoder), one without about 250 µs: on the call 1.6
instead of 2.7 % of a core. The noise suppression also allocates less (618–627 instead of 1027
times per frame, from the pool); the de-esser takes 1–3 µs more without the zeros, the other stages
the same.

## Steady tones

Whether a filter for steady tones before the noise suppression is needed: the built-in microphone
sits next to the laptop's fans, and before the high-pass moved first, DeepFilterNet3's estimate rose
to about 0 dB after 10 s of a pause and let their tones through (see the concept, "Tones"). Found
with `scripts/tones.py` (spectra of 8192 samples every 100 ms, a tone more than 12 dB above the
median within ±100 Hz):

- *Fans:* on two recordings of the built-in microphone 6–9 tones between 1.06 and 1.68 kHz, 13–22 dB
  above the noise around them, at −62 to −71 dBFS (pauses at −51 dBFS), in the pauses and in the
  speech alike. Steady within a recording (moving about 1 Hz per 100 ms, the median per 10 s within
  ±1 Hz over 100 s), not between them (1559 and 1670 Hz on one, 1506 and 1623 Hz on the other).
  On the call (20 min, pauses at −44 dBFS) none: its noise is broadband.
- *Electrical:* exact multiples of 833.3 Hz (2.5, 4.17, 5, 6.67, 7.5, 8.33, 10 kHz) on the first
  two recordings, of 1 kHz (7, 8, 10, 11 kHz) on the call, at −68 to −88 dBFS, steady to 1 Hz.

Taken out with `tones.py --remove` (notches of Q 30 at every tone of the pauses, zero phase) and
traced as above, on the recording with the fans:

| Built-in microphone, fans      | noise in pauses | median | estimate > 0 dB |
| ------------------------------ | --------------: | -----: | --------------: |
| with the zeros                 |           4.9 % |  −120 |           2.0 % |
| with the zeros, tones removed  |           5.0 % |  −120 |           1.8 % |
| without them                   |          74.4 % |  −44.9 |           2.0 % |
| without them, tones removed    |          74.4 % |  −44.7 |           1.8 % |

The estimate in the minute of silence (20–85 s) averages −13.6 dB with the tones and without, 19 of
its 6500 frames above 0 dB in both. On the call the same (1.4 and 1.5 %, 72.1 and 72.0 %). With the
high-pass first and the input gain the tones don't open the model anymore; what comes through
without the zeros is broadband (the eight strongest tones are 1–2 % of the power of its pauses, of
the raw microphone's 11–14 %).

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

The other stages and the maxima per frame (1.2–1.9 ms, from `denoise`) stayed the same. Intel
CPUs take longer over a denormal than this Zen 4, so it may save more there. Hence
`dsp::without_denormals` (flush to zero and denormals are zero, the `no_denormals` crate) around
every frame of the chain.

Flushing them broke DeepFilterNet3 in long digital silence (upstream issue #713): its unit norm
divides each bin by the root of a running mean of its magnitude, which only decays (×0.99 per frame)
while the input is exact zeros, from 1e-4 at the highest bins. Flushed, it reaches 0 after 7,774
frames (77.7 s) and `0 / 0` gave NaN; the chain put out NaN from frame 7,773 on and kept doing so
after sound came back (the filters after `denoise` hold it in their state). Upstream's guard, which
should skip the model after 5 frames below −70 dBFS (mean square below 1e-7), never fired: a frame
with zero gains reset its counter. Fixed both (a state of 0 gives a bin of 0, the counter goes by the
level alone). On the recordings the outputs stayed bit for bit the same on the built-in microphone
and with the speakers; on the USB microphone, whose pauses go below −70 dBFS after the high-pass,
3,559 of 10,243 and 24,148 of 61,248 frames differ (the model's state now holds through such
pauses), by −59 and −67 dBFS against the signal's −24 and −25 dBFS: the speech frames (above
−40 dBFS) changed by −0.02 and −0.04 dB on average (1st percentile −1.1 and −0.7 dB), the pause
frames above −60 dBFS went from 289 to 299 and from 3,522 to 3,346.

## Release profile

On the same recording and machine. Fat LTO with one codegen unit: `cargo bench --bench stages`
202 and 196 µs per frame (without: 203 and 199 µs), the binary 39 instead of 52 MB, but 7 minutes
for a build from scratch and a full relink after every change, so it's not used. `panic = "abort"`
(benches ignore it, hence the CPU time of `voice-filter process` over the whole recording, five
runs each, alternating): 12.37 s against 12.40 s with unwinding, the output bit for bit the same,
the binary 46.6 instead of 52.2 MB; it's in the profile for how a panic ends the daemon, not for
speed.

### Target CPU

What `cargo install` builds runs on every x86-64 CPU (SSE2); tract picks its matrix kernels at
runtime anyway (AVX-512 ones on this machine in every build), the rest of the code doesn't. The
benchmark built four ways (`RUSTFLAGS`, each in a target directory of its own) at commit
`38afbed`, on the call recording (61,248 frames), an AMD Ryzen 7 8840U (Zen 4) idle after a
reboot: five rounds, each running the four binaries one after the other, the median of the five
means per frame (each the best of the benchmark's 5 passes) and their range, in µs:

| Stage      |               Baseline |           `+avx2,+fma` |    `target-cpu=native` |            `x86-64-v4` |
| ---------- | ---------------------: | ---------------------: | ---------------------: | ---------------------: |
| highpass   |       1.53 (1.49–1.53) |       1.21 (1.18–1.22) |       1.21 (1.18–1.21) |       1.20 (1.18–1.22) |
| echo       |       5.26 (5.13–5.29) |       5.17 (4.98–5.27) |       5.30 (5.09–5.38) |       5.40 (5.33–5.50) |
| denoise    | 145.88 (142.23–147.73) | 135.48 (134.08–136.11) | 140.89 (137.30–141.59) | 137.52 (135.60–138.76) |
| eq         |       1.22 (1.20–1.23) |       1.22 (1.19–1.22) |       2.48 (2.43–2.49) |       1.24 (1.23–1.26) |
| deesser    |       7.48 (7.31–7.55) |       5.68 (5.54–6.16) |       5.09 (4.98–5.11) |       5.80 (5.70–6.19) |
| agc        |       0.60 (0.60–0.62) |       0.54 (0.53–0.56) |       0.39 (0.38–0.40) |       0.39 (0.39–0.39) |
| compressor |       2.98 (2.91–3.00) |       2.20 (2.15–2.22) |       2.37 (2.31–2.38) |       2.27 (2.23–2.28) |
| expander   |       0.43 (0.43–0.44) |       0.41 (0.40–0.43) |       0.40 (0.39–0.44) |       0.43 (0.42–0.45) |
| limiter    |       3.60 (3.50–5.99) |       8.66 (8.42–8.70) |       2.00 (1.96–2.33) |       2.17 (2.13–2.52) |
| total      | 170.41 (164.79–173.37) | 160.58 (158.47–161.78) | 160.40 (156.03–160.85) | 156.39 (154.21–158.48) |

`x86-64-v4` saved 8 %, `native` and `+avx2,+fma` 6 % each, 10–14 µs per frame, about 0.1 % of a
core. Most of it is in `denoise`, where `perf` put the difference in tract's depthwise
convolution: for input that isn't contiguous it runs a scalar loop compiled for the baseline (see
`TODO.md`), faster when the compiler may use AVX, though still one float per instruction. Our own
stages differ by stage: the de-esser's FIR, the compressor and the high-pass gain from AVX in
every variant, the limiter's peak loop takes 2.4 times as long with AVX2 alone and less with
AVX-512, the EQ's biquads twice as long with `native` (tuned for Zen 4, which uses 512-bit
registers throughout, where `x86-64-v4` mostly keeps to 256 bits). So none of these is set for
the published crate: it would leave out older CPUs for little, and a `target-cpu` in a
`.cargo/config.toml` of the repository wouldn't reach `cargo install` from crates.io anyway (only
`cargo install --path .`). Earlier runs on a busy machine (another build running) had found
`native` no faster than the baseline and `+avx2,+fma` 15 µs faster in `denoise`: differences of
that size need an idle machine.

## Allocations of the noise suppression

`cargo bench --bench stages` counted about 820 allocations per frame in `denoise`, all in tract
running the model (`DfTract`'s own code keeps its buffers). Counted per node with a hook in
tract's `run_plan_with_eval` (the three parts on every frame, random inputs, so 1,290 per frame;
in the chain the decoders are skipped on some frames):

| Where                                          | per frame |
| ---------------------------------------------- | --------: |
| `OptMatMulPack`, packing the matrix operands   |       658 |
| `SimpleState::run` itself (symbols, values)    |       265 |
| `OptMatMul`, its outputs                       |       134 |
| `Scan` (the GRUs)                              |        95 |
| `DepthWiseConv`                                |        43 |
| the rest (reshapes, pads, delays, activations) |        95 |

tract 0.23.9 has no arena, pool or outputs written in place, so it can't run on preallocated
tensors. With glibc the allocations didn't fault in new pages (7,616 page faults over 200 frames,
7,175 over 5,200: it reuses what was freed, no `mmap`/`brk` per frame); `malloc` and `free`
took 1.5 % of the samples of `perf`, with glibc's internals up to about 4 %. What's against them
in the audio path is glibc's locks and the unbounded time, not the CPU.

Since then the audio path allocates from the pool of `rt_alloc` (TLSF): the whole chain takes at
most 289–297 KB of it, 147 KB of it for good (tract's scratch space and the states of its ops),
and none of it goes to the system allocator (`tests/rt_alloc.rs`). `cargo bench --bench stages`
on the speech-like signal, three runs each, alternating (the `x86-64-v4` build, on a busy
machine, so only the comparison counts): `denoise` 221, 229 and 225 µs per frame from the pool,
227, 222 and 229 µs from glibc.

## Quantum

Whether a quantum of 480 samples, a frame, lowers the latency: PipeWire rounds the 480 of
`node.latency` down to 256, so the bridge waits for almost a frame (448 samples at least) before the
chain can run. Forced at runtime with `pw-metadata -n settings 0 clock.force-quantum 480` (and back
with 0), the built-in microphone, PipeWire 1.6.10, `pw-record --target voice-filter` recording for
10 s, the parts from the daemon's `Latency` property, three readings per run, each the same:

| Quantum      | Microphone | Bridge | Chain |  Total |
| ------------ | ---------: | -----: | ----: | -----: |
| 256          |        6.0 |    9.3 |  37.0 |   52.3 |
| 480, a run   |       10.7 |   15.3 |  37.0 |   63.0 |
| 480, another |       10.7 |    5.3 |  37.0 |   53.0 |

What the bridge saves at 480 (4 ms at best) the microphone loses (4.7 ms, the 224 samples the
quantum grew by), and in some runs the bridge settled a frame higher, depending on the order of the
callbacks at the start. At 256 the bridge sits at its minimum already. So the quantum stays as
PipeWire chooses it, neither forced by the daemon nor through `clock.power-of-two-quantum = false`.

## Live CPU load

What the chain takes in the daemon, from the app's Processing Time (`FrameTimes`, the last 10 s),
2026-11-05, the daemon from `cargo install` on this machine (so the `x86-64-v4` build, see
[Target CPU](#target-cpu)), an AMD Ryzen 7 8840U (8 cores, 16 threads), quantum 256, in ms per
frame:

| Reading                               | Part    | Mean |  95 % |  99 % | Longest |
| ------------------------------------- | ------- | ---: | ----: | ----: | ------: |
| echo switched off                     | total   | 0.59 |  1.31 |  1.62 |    2.08 |
|                                       | denoise | 0.54 |  1.26 |  1.59 |    2.03 |
|                                       | other   | 0.05 |  0.07 |  0.08 |    0.14 |
| echo on, speech about all of the time | total   | 0.68 |  1.39 |  1.93 |    2.23 |
|                                       | echo    | 0.14 |  0.22 |  0.27 |    0.45 |
|                                       | denoise | 0.50 |  1.20 |  1.70 |    1.99 |
|                                       | other   | 0.04 |  0.07 |  0.08 |    0.10 |

0.68 ms per 10 ms is 6.8 % of a core, 0.85 % of the 8 cores. That's about four times the benchmark
(`denoise` 0.14 ms per frame with the zeros, the chain 0.16 ms, see [Zero mask](#zero-mask)), with
less speech in it, but most likely also because the benchmark runs frame after frame, with warm
caches and the clock up, while the daemon runs one frame every 10 ms and the core sleeps and clocks
down in between. The benchmark has no reference for the echo canceller, hence its 5 µs for `echo`
against 0.14 ms live. So the benchmark compares stages and builds, the CPU load users see is this
one.

## AEC3 tuning in double talk

Whether AEC3's own configuration (sonora 0.3.0, `echo_canceller3_config`) keeps more of the voice
while you talk into the playback than Voice Protection. 2026-11-07, `examples/sidechain.rs` with
`--aec3` (the settings in its docs), the Razer on the laptop's video with speech (`--voice 0..21
--echo 60..85`, also with `--echo-gain 12`) and on the Bluetooth music (`--voice 67..90`, without
vocals `--echo 30..61 --echo-gain 10`, the song with vocals `--echo 193..238`), `--mix 1,0.75
--speech-db 10`. *Echo* is what's left of the echo alone (dBFS), *loss* the mean loss of the voice
(dB), *lost* the share of it more than 6 dB down, *gaps* what comes through besides the voice (dB
against it), all as in the concept's [Double talk](concept.md#double-talk-the-echo-cancellation-only-decides).
*Now* is the chain today (AEC3's defaults, Voice Protection 25 % while you talk: `adapt+10-0.75-nodf`),
*AEC3 alone* without Voice Protection (`mix-1.00`), *tuned* with
`lf=6/8+hf=2/4+enr=1`: AEC3 takes you for dominant at an echo-to-near-end ratio below 1 instead of
0.25 and then leaves the bands alone up to a ratio of 6 (low) and 2 (high) instead of 1.09 and 0.1.

| Echo               | Variant                 |  Echo | Loss | Lost |  Gaps |
| ------------------ | ----------------------- | ----: | ---: | ---: | ----: |
| Video, speech      | now                     | −88.7 |  6.3 | 29 % | −33.4 |
|                    | AEC3 alone              | −94.6 |  6.5 | 29 % | −41.8 |
|                    | tuned, alone            | −94.6 |  3.7 | 16 % | −39.3 |
|                    | tuned, Voice Protection | −88.7 |  4.9 | 20 % | −30.8 |
| Video, +12 dB      | now                     | −78.8 |  5.6 | 25 % | −28.6 |
|                    | AEC3 alone              | −78.8 |  5.5 | 24 % | −36.9 |
|                    | tuned, alone            | −78.8 |  4.3 | 20 % | −36.0 |
|                    | tuned, Voice Protection | −78.8 |  4.9 | 21 % | −26.2 |
| Music, no vocals   | now                     | −87.6 | 13.4 | 53 % | −26.6 |
|                    | AEC3 alone              | −87.6 | 16.1 | 63 % | −32.4 |
|                    | tuned, alone            | −86.2 | 10.8 | 44 % | −32.4 |
|                    | tuned, Voice Protection | −86.2 |  9.4 | 38 % | −25.8 |
| Song with vocals   | now                     | −67.8 | 15.0 | 57 % | −31.2 |
|                    | AEC3 alone              | −65.9 | 17.0 | 66 % | −39.0 |
|                    | tuned, alone            | −65.9 | 13.7 | 53 % | −33.5 |
|                    | tuned, Voice Protection | −67.8 | 12.9 | 48 % | −28.7 |

- Tuned AEC3 alone keeps more of the voice than the chain today on all four and lets less through
  besides it (gaps 2–7 dB lower), the echo alone stays where it was: the tuning only acts while
  AEC3 takes you for dominant. Voice Protection on top keeps a little more on the music, a little
  less on the video, and raises the gaps by 5–10 dB again.
- The single settings: `lf=3/4+hf=1/2` took 0.5–1.3 dB off the loss, `6/8` and `2/4` up to 2 dB,
  `10/12` and `4/6` no more than that; `enr=1` alone 0.3–2.2 dB, together with `6/8`, `2/4` 1.2–5.3
  dB. `snr=10` and `hold=100` hardly changed anything; `trigger=6` (6 bands instead of 12) let 5 dB
  more of the louder video's echo alone through and was left out; the echo path's default gain
  halved (`ep=0.5`) changed nothing.
- The losses with AEC3's defaults are lower than those in the concept's first table (video 6.5
  instead of 8.0 dB, music 16.1 instead of 10.7 dB with another section and level), measured with
  sonora 0.2.0 on other sections; not compared on the same ones.
- Not listened to yet; a double talk built from two takes, not a real one.

## Open

- The zero mask: measured again (above), still to be listened to, with the expander's `level` at
  −40 dBFS too.
- A high-pass between 40 and 80 Hz in front, which may cost less on VoiceBank-DEMAND and still keep
  the rumble from the model.
