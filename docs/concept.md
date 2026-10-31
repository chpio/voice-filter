# Concept: Live Voice Filter for Linux

Started as the German document [Konzept: Live-Sprachfilter für Linux](https://claude.ai/artifact/2tXoWyCXGXVbrA8qy7i9kN) (as of 2026-10-31); this file is now the authoritative version, the original isn't updated anymore.

## Goal and requirements

A live voice filter for Linux that shows up in PipeWire as a virtual microphone. Any app (Discord, browser, OBS) uses it like a regular microphone.

- **Live:** total latency below 40 ms if possible, so conversations stay natural.
- **Features:** remove echo, remove noise, optionally suppress other voices, enhance the voice.
- **Hardware:** runs entirely on the CPU. The GPU is only an optional accelerator for heavy add-on stages.
- **Platform:** Arch Linux, GNOME on Wayland, PipeWire.
- **Operation:** stages and modes can be switched at runtime, without clicks.

## Signal chain

The signal passes through all stages in a fixed order. Echo comes first, because otherwise the other stages treat the echo as speech.

1. **Microphone** – hardware input, 48 kHz mono
2. **Remove echo** – WebRTC AEC3 via the PipeWire module (see [Echo cancellation](#echo-cancellation))
3. **Remove noise** – DeepFilterNet3, 48 kHz, 30 ms latency (main filter)
4. **Suppress other voices** (optional) – Hush, modes Off · Full · Gate, about 11 ms more
5. **Enhance the voice** – high-pass, EQ, de-esser, compressor, AGC, limiter
6. **Restoration** (optional, later) – ML model such as Stream.FM, GPU required
7. **Virtual microphone "Voice Filter"** – selected in Discord, the browser or OBS

Voice enhancement comes after denoising, so the compressor doesn't bring up the noise.

## Model choice

DeepFilterNet3 is the main filter. It's the strongest open live option with the full frequency range. Hush only complements it for other voices. There's no independent comparison of all models for 2026; Hush's placement on the Hugging Face leaderboard comes from its maker.

| Model                                                       | Sample rate | Strength                                                 | Weakness                           | Role                       |
| ----------------------------------------------------------- | ----------- | -------------------------------------------------------- | ---------------------------------- | -------------------------- |
| [DeepFilterNet3](https://github.com/Rikorose/DeepFilterNet) | 48 kHz      | Best live quality, core in Rust, LADSPA plugin available | 30 ms latency, somewhat more CPU   | Main filter                |
| [Hush](https://huggingface.co/Aliados/hush)                 | 16 kHz      | Also suppresses other voices, 8 MB, under 1 ms per frame | Sounds like a phone, young project | Optional                   |
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

We start with PipeWire's `libpipewire-module-echo-cancel` (WebRTC AEC3). It creates a sink, through which the playback goes and which gives it the reference (or, with `monitor.mode`, it listens to the default sink's monitor), and a source with the echo-cancelled microphone, which voice-filter reads instead of the microphone. Its own noise suppression and gain control must be off, so they don't interfere with DeepFilterNet3.

- [ ] TODO: evaluate echo cancellation in our own process (monitor of the output device plus microphone, WebRTC AEC3 through a Rust binding): one node less in the graph and full control over latency and order, but more code. Decide after measuring the module's latency in phase 3.

## Suppressing other voices

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

## Voice enhancement

Classic DSP brings the biggest audible gain and runs live with practically no CPU load. ML restoration is a later, optional stage.

**Level 1: classic DSP (part of the first version)**

1. High-pass around 80 Hz against rumble, footfall noise and plosives
2. EQ: slight cut around 250 Hz against muddiness, boost around 3.5 kHz for presence
3. De-esser against harsh S sounds
4. Compressor for an even volume
5. Automatic gain control (AGC)
6. Limiter against clipping

The values are starting points, later adjustable as presets.

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
  - The quantum belongs to the whole graph: the default here is 1024 samples (21 ms), `node.latency = 480/48000` lowers it, but another node can lower it further (256 in the spike). The process callback has to produce `buffer.requested()` samples (feature `v0_3_49`), not the buffer size, otherwise the buffer adds about 250 ms. A FIFO collects the samples into 480-sample frames; if the quantum doesn't fit 480, this adds up to one frame (10 ms) of latency.
  - The capture stream and the virtual microphone go into one `node.group`, like `module-loopback` does, so both run in the microphone's cycle; a virtual source has no clock of its own.
- **Stages:** a common trait with name, latency and processing of one frame. Stages are interchangeable and optional.
- **Resampling 48 ↔ 16 kHz:** fixed filter with factor 3. Known, constant latency, no allocation in the audio path.
- **Control:** a GNOME app (GTK4 with libadwaita via `gtk4-rs`/`libadwaita-rs`) as a separate process; the audio process keeps running when the window is closed. They talk over the D-Bus session bus (`zbus`), which also allows a command line (`busctl` or our own `voice-filter ctl`). Inside the audio process the settings are shared lock-free with the audio path.
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

- Latency budget: no fixed number up front. Phase 1 measures the latency per stage, the budget follows from it and decides whether Hush and the low-latency variant of DeepFilterNet3 are needed. Measured so far: DeepFilterNet3 delays by 30 ms (10 ms STFT overlap plus 2 lookahead frames of 10 ms, `DfTract::delay`), not 20 ms as first assumed; with the FIFO (up to 10 ms) and the PipeWire buffers the target of 40 ms is out of reach with DeepFilterNet3 alone, about 45–60 ms are realistic. The low-latency variant without lookahead would save 20 ms.
- Echo cancellation: PipeWire module first, our own process as a TODO (see [Echo cancellation](#echo-cancellation)).
- Control: GNOME app over D-Bus.

**Open questions:**

- [ ] Presets for voice enhancement (e.g. "Call", "Stream", "Podcast")?
- [ ] Default attenuation limit of DeepFilterNet3, to be chosen by listening in the GNOME app (it can change at runtime). Unlimited (the model's default) sounds dull: in a first test recording the speech above 1 kHz lost about 20 dB, with a limit of 12 dB about 11 dB, while the pauses lost 13 and 8 dB. That recording clipped (0.16 % of the samples), which may account for part of it.
- [ ] Should the chain also filter incoming audio (speaker side)?
- [ ] Does the low-latency variant of DeepFilterNet3 lower the latency noticeably, and at what cost?
- [ ] Can the weights be stored as fp16 or quantized to int8 (about half or a quarter of the size) without audible loss, and does `tract` support it for all operators?

## Roadmap

Four phases, each usable on its own. Each phase ends with a latency and listening test.

1. **Base:** vendor `deep-filter-rt`, PipeWire node, DeepFilterNet3, offline mode for WAV tests, latency measurement per stage.
2. **Voice:** DSP chain (high-pass, EQ, de-esser, compressor, AGC, limiter), presets, D-Bus interface and GNOME app; the attenuation limit of DeepFilterNet3 (`--atten-lim` so far) as one of the first runtime settings.
3. **Echo and other voices:** integrate the echo-cancel module, Hush with Full and Gate modes, click-free switching.
4. **Restoration:** evaluate Stream.FM or similar, GPU backend for AMD, optional as a stage.

After that: a package for Arch (AUR).
