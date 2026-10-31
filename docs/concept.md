# Concept: Live Voice Filter for Linux

Started as the German document [Konzept: Live-Sprachfilter für Linux](https://claude.ai/artifact/2tXoWyCXGXVbrA8qy7i9kN) (as of 2026-10-31); this file is now the authoritative version, the original isn't updated anymore. How to use it is in the [README](../README.md).

**Status** (2026-10-31): phase 1 is done and most of phase 2. The live filter runs as the virtual microphone "Voice Filter" with DeepFilterNet3, high-pass, EQ, de-esser, AGC, compressor and limiter, controlled from a GNOME app over D-Bus. Missing from phase 2: presets. See the [roadmap](#roadmap).

**Contents:** [Goal and requirements](#goal-and-requirements) · [Signal chain](#signal-chain) · [Model choice](#model-choice) · [Echo cancellation](#echo-cancellation) · [Suppressing other voices](#suppressing-other-voices) · [Voice enhancement](#voice-enhancement) · [GPU strategy](#gpu-strategy) · [Architecture](#architecture) · [Risks and open questions](#risks-and-open-questions) · [Roadmap](#roadmap)

## Goal and requirements

A live voice filter for Linux that shows up in PipeWire as a virtual microphone. Any app (Discord, browser, OBS) uses it like a regular microphone.

- **Live:** total latency below 40 ms if possible, so conversations stay natural.
- **Features:** remove echo, remove noise, enhance the voice; optionally suppress other voices (postponed, see [Suppressing other voices](#suppressing-other-voices)).
- **Hardware:** runs entirely on the CPU. The GPU is only an optional accelerator for heavy add-on stages.
- **Platform:** Arch Linux, GNOME on Wayland, PipeWire.
- **Operation:** stages and modes can be switched at runtime, without clicks.

## Signal chain

The signal passes through all stages in a fixed order. Echo comes first, because otherwise the other stages treat the echo as speech.

1. **Microphone** – hardware input, 48 kHz mono
2. **Remove echo** – WebRTC AEC3 in our own process, with the speakers' monitor as the reference (see [Echo cancellation](#echo-cancellation))
3. **Remove noise** – DeepFilterNet3, 48 kHz, 30 ms latency (main filter)
4. **Suppress other voices** (optional, postponed) – Hush, modes Off · Full · Gate, about 11 ms more
5. **Enhance the voice** – high-pass, EQ, de-esser, AGC, compressor, limiter
6. **Restoration** (optional, later) – ML model such as Stream.FM, GPU required
7. **Virtual microphone "Voice Filter"** – selected in Discord, the browser or OBS

Voice enhancement comes after denoising, so the compressor doesn't bring up the noise.

## Model choice

DeepFilterNet3 is the main filter. It's the strongest open live option with the full frequency range. Hush was meant to complement it for other voices, but is postponed after a first test (see [Suppressing other voices](#suppressing-other-voices)). There's no independent comparison of all models for 2026; Hush's placement on the Hugging Face leaderboard comes from its maker.

| Model                                                       | Sample rate | Strength                                                 | Weakness                           | Role                       |
| ----------------------------------------------------------- | ----------- | -------------------------------------------------------- | ---------------------------------- | -------------------------- |
| [DeepFilterNet3](https://github.com/Rikorose/DeepFilterNet) | 48 kHz      | Best live quality, core in Rust, LADSPA plugin available | 30 ms latency, somewhat more CPU   | Main filter                |
| [Hush](https://huggingface.co/Aliados/hush)                 | 16 kHz      | Also suppresses other voices, 8 MB, under 1 ms per frame | Sounds like a phone, young project | Postponed                  |
| [GTCRN](https://github.com/sherryyu33/gtcrn)                | 16 kHz      | Extremely small, 23.7 K parameters                       | Narrowband                         | Fallback for weak hardware |
| RNNoise                                                     | 48 kHz      | Minimal CPU usage                                        | Clearly worse quality              | Not planned                |

There's a pure Rust implementation of Hush ([hush-vani](https://crates.io/crates/hush-vani)) with embedded weights, without ONNX Runtime. It comes from a third party, not from the maker.

**DeepFilterNet3 vendored as `deep-filter-rt`:** the Rust crate of DeepFilterNet (`deep_filter`, folder `libDF`) is only usable from git: the last release on crates.io (0.2.5, July 2022) has neither DeepFilterNet3 nor the real-time inference, and the upstream repo has been dormant since September 2024. crates.io doesn't accept packages with git dependencies, so voice-filter couldn't be published that way. Instead we vendor it as the workspace crate `crates/deep-filter-rt` and publish it ourselves:

- Copied without its history from upstream commit `d375b2d` (2024-10-17): `libDF` and the DeepFilterNet3 model, into `crates/deep-filter-rt`; its `NOTICE.md` names the origin. The upstream history stays available on GitHub; taking it over would have brought about 470 foreign, unsigned commits without conventional commit prefixes and an older model version (8 MB) into this repo, while the code gets reworked heavily anyway. In the workspace a change of its API and the caller go into one commit; it can still be split off into a repo of its own later.
- The model is stored unpacked in `models/DeepFilterNet3/` (three ONNX files and `config.ini`) instead of upstream's `DeepFilterNet3_onnx.tar.gz`: the weights barely compress (gzip saves 7 %, zstd, xz or brotli at most 1 % more), git and crates.io compress anyway, `config.ini` stays readable in diffs and the crates for unpacking the archive can go. Not in Git LFS: the files hardly ever change, and LFS would break builds from clones without git-lfs (`include_bytes!` would embed the pointer files) and every mirror would need LFS.
- Reduced to the inference: the core (`lib.rs`) and the `tract` inference (`tract.rs`), about 1,700 of upstream's 7,900 lines and 10 of its 28 dependencies. Updated from tract 0.21 to 0.23, the output differs from upstream's by at most 1.6e-7 (−114 dB), i.e. rounding.
- The model (8.6 MB) stays embedded and fits the 10 MB limit of crates.io. The low-latency variant (36 MB) doesn't; if we need it, it has to be loaded at runtime (or shrunk, see the open questions).

## Echo cancellation

Echo cancellation needs the signal the speakers are playing as a reference: an adaptive filter learns how the room changes the sound on its way from the speakers to the microphone, predicts the echo from the reference and subtracts it; a second step suppresses the residual echo. With headphones there's no echo, the stage can stay off.

**Planned: in our own process.** Besides the microphone, voice-filter records the monitor of an output device, chosen in the app like the microphone (the default output by default), as the reference, and runs AEC3 as the first stage of the chain. It works on 10 ms frames at 48 kHz, our frames. Against PipeWire's `libpipewire-module-echo-cancel` (the same WebRTC AEC3): the speakers are chosen in the app and switched at runtime, nothing to configure and no playback to reroute (the module either creates a sink the playback has to go through, or with `monitor.mode` listens to the default output only, and changing it means editing its config and restarting PipeWire), no extra nodes with their own buffers, and the stage switches, has parameters and status like the others. The cost is more code (a second capture stream with a bridge into the audio path, the choice of the output) and the clocks of microphone and speakers, which drift against each other (AEC3 keeps estimating the delay; to be measured live).

AEC3 comes from [sonora](https://github.com/dignifiedquire/sonora) (BSD-3-Clause), a pure Rust port of WebRTC's audio processing (M145 plus later fixes, passes the C++ test suite), so no C++ library: only its echo canceller, its noise suppression and AGC2 stay off. The alternatives: [webrtc-audio-processing](https://github.com/tonarino/webrtc-audio-processing) wraps the C++ library (system package or built with meson); [aec3-rs](https://github.com/RubyBit/aec3-rs) is a less verified port with an API still changing; [DTLN-aec](https://github.com/breizhn/DTLN-aec) is a neural model at 16 kHz.

**Spike** (2026-10-31, sonora 0.2.0, AEC3 only, a recording of the USB microphone and of the speakers' monitor, playback for 24 s with the speaker talking into it from 7 s, then the speaker alone):

- Echo alone: 28–34 dB removed. The speaker alone, without playback: unchanged.
- Talking into the playback: the level of the output no longer follows the playback (correlation 0.04, the microphone 0.37). But DeepFilterNet3 alone did as well in this recording (0.00); in an earlier test it let speech from a phone through, so a test with only speech played back is still to do.
- 60 µs per frame on average, at most 0.6 ms; flushing denormals changed nothing here. About 40 allocations per frame, which have to go first (see `TODO.md`).
- AGC2 against our AGC, compressor and limiter, on another recording: 6 dB quieter with its defaults, the same spread between quiet and loud speech, pauses 5 dB further below the speech, peaks up to 0 dBFS. Not better enough to give up our adjustable chain; the expander would bring the quieter pauses.

## Suppressing other voices

**Postponed** (2026-10-31): in a first test Hush didn't separate the voices, DeepFilterNet3 alone did better (see [Test](#test) below). The plan as it was:

The feature can be enabled at runtime and offers two modes. Hush costs no license fees (Apache 2.0); the costs are latency, sound and maintenance.

| Mode | How it works                                                                 | Advantage                                         | Disadvantage                                               |
| ---- | ---------------------------------------------------------------------------- | ------------------------------------------------- | ---------------------------------------------------------- |
| Off  | Hush doesn't run                                                             | No CPU load                                       | No suppression                                             |
| Full | Hush's output replaces the signal                                            | Strongest suppression, even when speaking at once | Everything above 8 kHz is missing, sounds like a phone     |
| Gate | Hush runs in parallel and only provides a gain per 10 ms for the 48 kHz path | Full sound                                        | Other voices mainly removed in speech pauses; experimental |

**Costs in detail:**

- **CPU:** under 1 ms per 10 ms frame, barely noticeable next to DeepFilterNet3.
- **Latency:** about 11 ms extra (Hush 10 ms plus resampling from 48 to 16 kHz and back). All modes get the same latency, so switching with a crossfade stays click-free.
- **Development:** fixed 3:1 resampling, crossfade when switching, smoothing of the gate gain (open fast, close slowly).
- **Maintenance:** a project of a company with a different core business, version 2 announced. The model format may change.

If you never need the feature, leave the stage out entirely and save the 11 ms.

### Test

A recording from the USB microphone (speech at about −36 dBFS RMS, rather quiet): first the speaker alone, then a video with speech from a phone about a meter away, both at once. Run offline through DeepFilterNet3, hush-vani 0.1.2 (Full, and Gate with a gain per 10 ms from Hush's output against its input) and combinations, compared by ear and by the level per second against the input:

- DeepFilterNet3 alone keeps the speaker (−2 to −8 dB) and lets the video through while it plays alone; while the speaker talks, it suppresses the video by itself, and sounds good.
- Hush after DeepFilterNet3 swallowed the speaker alone (−27 to −65 dB). Hush depends on the level: 10 dB more before it keeps the speaker (still 5–10 dB below DeepFilterNet3 alone), but then attenuates speaker and video alike. With AGC and limiter before it (Hush last) it attenuates everything by 12–15 dB, again without separating: the AGC evens out the level difference Hush seems to rely on.
- Before trying again: a recording with the microphone close (speech peaks around −10 dBFS), which also helps every other stage. Hush alone at the original level separated as little.

Separating by the speaker's own voice (target speaker extraction or personalized speech enhancement, conditioned on an enrollment of a few seconds of the voice) would be the actual solution. Open options as of 2026-10:

- [VoxLock](https://github.com/arbazsiddiqui/voxlock): a 27 M parameter causal band-split RNN (from the REAL-TSE baseline), ONNX, 16 kHz, 24 ms plus chunk latency; but 60–110 % of a core by its own numbers (DeepFilterNet3 needs 2.5 %), and no license stated for the weights.
- Personalized DeepFilterNet2 ([arXiv 2404.08022](https://arxiv.org/abs/2404.08022)), Personalized PercepNet, pDCCRN: papers without released weights. DeepFilterNet3 with a speaker embedding would fit the chain best, but means training it ourselves.
- [WeSep](https://github.com/wenet-e2e/wesep): a toolkit for target speaker extraction, no pretrained models released yet.

## Voice enhancement

Classic DSP brings the biggest audible gain and runs live with practically no CPU load. ML restoration is a later, optional stage.

**Level 1: classic DSP (part of the first version)**

| Order | Stage      | Purpose                                 | Implemented as                                                                                                                                  |
| ----: | ---------- | --------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
|     1 | High-pass  | Rumble, footfall noise, plosives        | 80 Hz, 4th order Butterworth (two biquads)                                                                                                      |
|     2 | EQ         | Less muddiness, more presence           | −3 dB at 250 Hz, +3 dB at 3.5 kHz (peaking, Q 1)                                                                                                |
|     3 | De-esser   | Harsh S sounds                          | band above 5 kHz turned down while its share of the energy is above −7 dB (ratio 4, at most 12 dB), split with a linear-phase FIR, 1 ms latency |
|     4 | AGC        | A constant level, whatever the distance | −18 dBFS RMS, gain −12…+30 dB, rises 6 dB/s, falls 30 dB/s, holds below −55 dBFS                                                                |
|     5 | Compressor | Even loudness within words              | 3:1 above −24 dBFS, soft knee 6 dB, attack 5 ms, release 120 ms                                                                                 |
|     6 | Limiter    | No clipping                             | −1 dBFS, 1 ms lookahead, release 50 ms                                                                                                          |

The AGC comes before the compressor (the first draft had it the other way round): the compressor then sees a known level, so its threshold and make-up gain can be fixed relative to the AGC's target. The de-esser goes after the EQ, whose presence boost brings out the S sounds, and before the AGC: it detects the S sounds relative to the total energy, so it doesn't depend on the level. The values are the defaults, each one adjustable at runtime from the GNOME app; presets are to follow.

**Level 2: ML restoration ("studio voice")**

Goal: remove reverb, reconstruct missing highs, repair compression artifacts. There's hardly anything mature that is both live and open. The best candidate is [Stream.FM](https://arxiv.org/html/2512.19442v1) (research paper, late 2025):

- Removes noise and reverb, adds highs, repairs codec artifacts.
- Latency 24 to 48 ms.
- Real time on an RTX 4080; on a Ryzen 7 9800X3D only barely (factor 0.88). Probably not on a laptop processor.
- Code open according to the paper; weights and license still unclear.

## GPU strategy

The base runs on the CPU only. A GPU only pays off for heavy models such as the ML restoration.

- **Why not for everything:** live audio comes in 10 ms frames. For such small models, the transfer to the GPU costs more than the computation itself.
- **Where it makes sense:** level 2 restoration (Stream.FM or similar), which doesn't run in real time on the CPU.
- **AMD:** via ONNX Runtime with ROCm or MIGraphX, or Rust-native via `burn` with the `wgpu` backend. Whether Stream.FM runs on it needs to be checked.
- **Fallback:** if there's no suitable GPU, the stage is disabled. The rest of the chain runs unchanged.

## Architecture

A single Rust process as a PipeWire node. All stages run inside it, so the latency stays under control.

- **Language:** Rust. DeepFilterNet3 (as `deep-filter-rt`) and hush-vani are already Rust, PipeWire via `pipewire-rs`.
- **PipeWire:** reads from the microphone (or from the echo-cancel source) and provides "Voice Filter" as a virtual microphone. The process creates the device itself, so it only exists while voice-filter runs; if it isn't running, some apps silently switch to the unfiltered microphone.
- [ ] TODO: offer a persistent device as an option (virtual source declared in `~/.config/pipewire/pipewire.conf.d/`, voice-filter only writes into it): always present and silent instead of unfiltered without voice-filter, but needs an installed config file and possibly one more buffer of latency.
- **Frames:** internally 48 kHz, 10 ms per frame (480 samples). Fits DeepFilterNet3 (hop 480) and Hush (hop 160 at 16 kHz) exactly.
- **PipeWire details** (from a spike with PipeWire 1.6.10, WirePlumber 0.5.18 and `pipewire` 0.10):
  - The virtual microphone is a stream with `media.class = Audio/Source`, not `Audio/Source/Virtual`: WirePlumber never sets a format for the latter, so it gets no ports and no app can record from it (the same happens to `pw-loopback`'s virtual sources).
  - The quantum belongs to the whole graph: the default here is 1024 samples (21 ms), `node.latency = 480/48000` lowers it, but PipeWire rounds it down to a power of two (`clock.power-of-two-quantum`, true by default), so it runs at 256. With 256 the bridge needs at least 448 samples (9.3 ms) to turn the blocks into 480-sample frames, measured are 11.3 ms; with `clock.power-of-two-quantum = false` (a system-wide setting in `pipewire.conf.d`) it would be 0–10 ms, depending on the order of the callbacks. The process callback has to produce `buffer.requested()` samples (feature `v0_3_49`), not the buffer size, otherwise the buffer adds about 250 ms. A FIFO collects the samples into 480-sample frames; if the quantum doesn't fit 480, this adds up to one frame (10 ms) of latency.
  - The capture stream and the virtual microphone go into one `node.group`, like `module-loopback` does, so both run in the microphone's cycle; a virtual source has no clock of its own.
- **Stages:** a common trait with name, latency and processing of one frame. Stages are interchangeable and optional. Each can be switched off at runtime: its input, delayed by its latency, plays instead, crossfaded over one frame, so the total latency never changes; a switched off stage doesn't process and saves its CPU.
- **Resampling 48 ↔ 16 kHz:** fixed filter with factor 3. Known, constant latency, no allocation in the audio path.
- **Control:** a GNOME app (GTK4 with libadwaita via `gtk4-rs`/`libadwaita-rs`, `voice-filter ui`) as a separate process; the audio process keeps running when the window is closed. They talk over the D-Bus session bus (`zbus`, name `net.b128.VoiceFilter.Daemon`), which also allows a command line (`busctl` or our own `voice-filter ctl`). Inside the audio process the settings are shared lock-free with the audio path.
- **Test mode:** the same chain processes WAV files offline. This allows comparing models and settings and measuring real-time capability.

## Risks and open questions

The biggest risk is latency: all stages together must stay live.

| Risk                                                                          | Consequence                         | Countermeasure                                                  |
| ----------------------------------------------------------------------------- | ----------------------------------- | --------------------------------------------------------------- |
| Latency adds up (DFN3 30 ms, FIFO up to 10 ms, Hush +11 ms, PipeWire buffers) | Conversations feel delayed          | Measure and show latency per stage; stages can be left out      |
| Gate mode cuts off quiet speech of your own                                   | Syllables go missing                | Open fast, close slowly; adjustable floor                       |
| hush-vani is a third-party project                                            | Maintenance uncertain               | Encapsulate behind an interchangeable interface; ONNX as plan B |
| DeepFilterNet3 outdated on crates.io, upstream dormant since 2024             | Real-time part only usable from git | Vendored as `deep-filter-rt`, published ourselves               |
| Stream.FM: weights, license, ROCm unclear                                     | Level 2 may not be feasible         | Check first, then decide                                        |

**Decided:**

- Latency budget: no fixed number up front. Phase 1 measures the latency per stage, the budget follows from it and decides whether Hush and the low-latency variant of DeepFilterNet3 are needed. Measured so far: DeepFilterNet3 delays by 30 ms (10 ms STFT overlap plus 2 lookahead frames of 10 ms, `DfTract::delay`), not 20 ms as first assumed; with the FIFO (up to 10 ms) and the PipeWire buffers the target of 40 ms is out of reach with DeepFilterNet3 alone. Measured live (USB microphone, quantum 256): microphone 8.0 ms (as PipeWire reports it) + bridge 11.3 ms + chain 31.0 ms (DeepFilterNet3 and limiter) = 50.3 ms until the app, which buffers on top of that (the de-esser added 1 ms to the chain since); at most 2 ms of processing per 10 ms frame, no dropouts. The low-latency variant without lookahead would save 20 ms, a quantum of 480 up to another 11 ms (see PipeWire details).
- Echo cancellation: PipeWire module first, our own process as a TODO (see [Echo cancellation](#echo-cancellation)).
- Control: GNOME app over D-Bus.

**Open questions:**

- [ ] Presets for voice enhancement (e.g. "Call", "Stream", "Podcast")?
- [ ] Default attenuation limit of DeepFilterNet3, to be chosen by listening in the GNOME app (it can change at runtime). Unlimited (the model's default) sounds dull: in a first test recording the speech above 1 kHz lost about 20 dB, with a limit of 12 dB about 11 dB, while the pauses lost 13 and 8 dB. That recording clipped (0.16 % of the samples), which may account for part of it.
- [ ] Should the chain also filter incoming audio (speaker side)?
- [ ] Does the low-latency variant of DeepFilterNet3 lower the latency noticeably, and at what cost?
- [ ] Personalized denoising that keeps only the user's voice: wait for an open model that runs in real time on little CPU, or train DeepFilterNet3 with a speaker embedding (see [Suppressing other voices](#suppressing-other-voices)).
- [ ] Can the weights be stored as fp16 or quantized to int8 (about half or a quarter of the size) without audible loss, and does `tract` support it for all operators?

## Roadmap

Four phases, each usable on its own. Each phase ends with a latency and listening test.

1. **Base** – done
   - [x] vendor `deep-filter-rt`
   - [x] PipeWire node with DeepFilterNet3
   - [x] offline mode for WAV tests
   - [x] latency measurement per stage
2. **Voice** – in progress
   - [x] high-pass, EQ, de-esser, AGC, compressor, limiter
   - [ ] presets
   - [x] D-Bus interface and GNOME app
   - [x] attenuation limit of DeepFilterNet3 as a runtime setting (its default is still open)
3. **Echo**
   - [x] spike: AEC3 from sonora on a recording (see [Echo cancellation](#echo-cancellation))
   - [ ] AEC3 as the first stage, the speakers' monitor as the reference, the output device chosen in the app
   - other voices (Hush with Full and Gate modes, or a personalized model) are postponed
4. **Restoration**
   - [ ] evaluate Stream.FM or similar, GPU backend for AMD, optional as a stage

After that: a package for Arch (AUR).
