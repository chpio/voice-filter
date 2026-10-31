# Concept: Live Voice Filter for Linux

As of: 2026-10-31 · Translation of the German document [Konzept: Live-Sprachfilter für Linux](https://claude.ai/artifact/2tXoWyCXGXVbrA8qy7i9kN); the original is authoritative.

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
2. **Remove echo** – WebRTC AEC3 via the PipeWire module
3. **Remove noise** – DeepFilterNet3, 48 kHz, 20 ms latency or more (main filter)
4. **Suppress other voices** (optional) – Hush, modes Off · Full · Gate, about 11 ms more
5. **Enhance the voice** – high-pass, EQ, de-esser, compressor, AGC, limiter
6. **Restoration** (optional, later) – ML model such as Stream.FM, GPU required
7. **Virtual microphone "Voice Filter"** – selected in Discord, the browser or OBS

Voice enhancement comes after denoising, so the compressor doesn't bring up the noise.

## Model choice

DeepFilterNet3 is the main filter. It's the strongest open live option with the full frequency range. Hush only complements it for other voices. There's no independent comparison of all models for 2026; Hush's placement on the Hugging Face leaderboard comes from its maker.

| Model                                                       | Sample rate | Strength                                                 | Weakness                                  | Role                       |
| ----------------------------------------------------------- | ----------- | -------------------------------------------------------- | ----------------------------------------- | -------------------------- |
| [DeepFilterNet3](https://github.com/Rikorose/DeepFilterNet) | 48 kHz      | Best live quality, core in Rust, LADSPA plugin available | At least 20 ms latency, somewhat more CPU | Main filter                |
| [Hush](https://huggingface.co/Aliados/hush)                 | 16 kHz      | Also suppresses other voices, 8 MB, under 1 ms per frame | Sounds like a phone, young project        | Optional                   |
| [GTCRN](https://github.com/sherryyu33/gtcrn)                | 16 kHz      | Extremely small, 23.7 K parameters                       | Narrowband                                | Fallback for weak hardware |
| RNNoise                                                     | 48 kHz      | Minimal CPU usage                                        | Clearly worse quality                     | Not planned                |

There's a pure Rust implementation of Hush ([hush-vani](https://crates.io/crates/hush-vani)) with embedded weights, without ONNX Runtime. It comes from a third party, not from the maker.

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

- **Language:** Rust. DeepFilterNet3 and hush-vani are already Rust, PipeWire via `pipewire-rs`.
- **PipeWire:** reads from the microphone (or from the echo-cancel source) and provides "Voice Filter" as a virtual microphone.
- **Frames:** internally 48 kHz, 10 ms per frame (480 samples). Fits DeepFilterNet3 (hop 480) and Hush (hop 160 at 16 kHz) exactly.
- **Stages:** a common trait with name, latency and processing of one frame. Stages are interchangeable and optional.
- **Resampling 48 ↔ 16 kHz:** fixed filter with factor 3. Known, constant latency, no allocation in the audio path.
- **Control:** settings shared lock-free, changes via a local socket, e.g. `voice-filter ctl speaker gate`. Later a GNOME app on top of it.
- **Test mode:** the same chain processes WAV files offline. This allows comparing models and settings and measuring real-time capability.

## Risks and open questions

The biggest risk is latency: all stages together must stay live.

| Risk                                                             | Consequence                         | Countermeasure                                                  |
| ---------------------------------------------------------------- | ----------------------------------- | --------------------------------------------------------------- |
| Latency adds up (DFN3 from 20 ms, Hush +11 ms, PipeWire buffers) | Conversations feel delayed          | Measure and show latency per stage; stages can be left out      |
| Gate mode cuts off quiet speech of your own                      | Syllables go missing                | Open fast, close slowly; adjustable floor                       |
| hush-vani is a third-party project                               | Maintenance uncertain               | Encapsulate behind an interchangeable interface; ONNX as plan B |
| DeepFilterNet3 outdated on crates.io                             | Real-time part only usable from git | Pin a fixed git revision                                        |
| Stream.FM: weights, license, ROCm unclear                        | Level 2 may not be feasible         | Check first, then decide                                        |

**Open questions:**

- [ ] What total latency is still acceptable for you?
- [ ] Echo cancellation via the PipeWire module or directly in our own process?
- [ ] Control: command line only, GNOME app, or both?
- [ ] Presets for voice enhancement (e.g. "Call", "Stream", "Podcast")?
- [ ] Should the chain also filter incoming audio (speaker side)?

## Roadmap

Four phases, each usable on its own. Each phase ends with a latency and listening test.

1. **Base:** PipeWire node, DeepFilterNet3, offline mode for WAV tests, latency measurement.
2. **Voice:** DSP chain (high-pass, EQ, de-esser, compressor, AGC, limiter), presets, runtime control.
3. **Echo and other voices:** integrate echo cancellation, Hush with Full and Gate modes, click-free switching.
4. **Restoration:** evaluate Stream.FM or similar, GPU backend for AMD, optional as a stage.

After that: a graphical interface for GNOME and a package for Arch (AUR).
