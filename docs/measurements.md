# Measurements

The numbers behind the defaults of the noise suppression and the order of the chain, with how they
were measured, so they can be repeated. The decisions themselves are in the [concept](concept.md).

**Contents:** [Method](#method) · [Skipping frames](#skipping-frames-by-the-estimated-snr) ·
[Low-latency variant](#low-latency-variant) · [High-pass and input level](#high-pass-and-input-level)
· [Zero mask](#zero-mask) · [Steady tones](#steady-tones) · [Open](#open)

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

## Open

- The zero mask: measured again (above), still to be listened to, with the expander's `level` at
  −40 dBFS too.
- A high-pass between 40 and 80 Hz in front, which may cost less on VoiceBank-DEMAND and still keep
  the rumble from the model.
