<img src="data/net.b128.VoiceFilter.svg" alt="" width="96" align="right">

# Voice Filter

**A live voice filter for Linux.** Voice Filter sits in PipeWire between your microphone and the
apps: every app (Discord, the browser, OBS) that records from the default input gets your voice
without the echo of your speakers and the background noise, with a clearer, even tone, and it
follows the microphone you choose in the system's sound settings. Or, if you'd rather choose the
apps, it shows up as a virtual microphone named _Voice Filter_ that only those apps use.

- **Removes the echo of your speakers** with WebRTC's echo canceller (AEC3), so calls work
  without headphones; it takes what the speakers play as a reference. While you talk into the
  playback it keeps more of your voice and leaves the rest of the echo to the noise suppression
- **Removes background noise** with [DeepFilterNet3](https://github.com/Rikorose/DeepFilterNet), a
  neural network that keeps the full 48 kHz range
- **Enhances the voice** with classic DSP: high-pass, EQ, de-esser, automatic gain control,
  compressor, an expander that keeps the noise down in speech pauses, and a limiter
- **Runs on the CPU**: the whole chain takes about 7 % of one core while you talk, under 1 % of an
  8-core CPU (measured live on a Ryzen 7 8840U: 0.7 ms per 10 ms frame on average, 1.9 ms at the
  99th percentile)
- **Switch and tune everything at runtime** from a GNOME app or over D-Bus, without clicks and
  without changing the latency; an icon in the tray switches the whole filter and opens the app
- **Test offline** with WAV files: the same chain, with the latency and processing time of every
  stage

> **Status:** early, 0.1.0 is the first release. Echo cancellation, noise suppression and voice
> enhancement work. What's still open is in the [concept](docs/concept.md#risks-and-open-questions).

## How it works

A single process reads the microphone, runs it through a chain of stages in 10 ms frames and
hands the result to the apps as the source _Voice Filter_:

```text
 Microphone ──▶ highpass ──▶ echo ──▶ denoise ──▶ eq ──▶ deesser ──┐
                             ▲                                     │
 Speakers ───────────────────┘ (what they play, as the reference)  │
                                                                   │
 ┌─────────────────────────────────────────────────────────────────┘
 └─▶ agc ──▶ compressor ──▶ expander ──▶ limiter ──▶ "Voice Filter" ──▶ apps
```

| Stage        | What it does                                                                      | Settings                               | Latency |
| ------------ | --------------------------------------------------------------------------------- | -------------------------------------- | ------: |
| `highpass`   | Removes rumble, footfall noise and plosives below 80 Hz, first of all             | `freq`                                 |       – |
| `echo`       | Removes the echo of the speakers (AEC3, from what they play), keeps the voice     | the speakers, `protect`                |    5 ms |
| `denoise`    | Removes background noise (DeepFilterNet3), hears a quiet voice raised to −35 dBFS | `limit`, `level`, `max-gain`           |   30 ms |
| `eq`         | Less muddiness (−3 dB at 250 Hz), more presence (+3 dB at 3.5 kHz)                | `low-mid`, `presence`, `presence-freq` |       – |
| `deesser`    | Tames harsh S sounds: turns down the band above 5 kHz while it dominates          | `freq`, `threshold`                    |    1 ms |
| `agc`        | Brings the voice to a constant level (−18 dBFS), holds it in pauses, hum, typing  | `target`, `max-gain`, `threshold`      |       – |
| `compressor` | Evens out the loudness within words (3:1 above −24 dBFS)                          | `threshold`, `ratio`                   |       – |
| `expander`   | Turns the speech pauses down by 20 dB, opens for speech (level or DeepFilterNet3) | `range`, `threshold`, `hold`, `level`  |       – |
| `limiter`    | Keeps the peaks below −1 dBFS                                                     | `ceiling`, `release`                   |    1 ms |

The values above are the defaults. The settings change them while it runs and are kept for the
next runs.

Every stage, and the whole filter, can be switched off while it runs: switching crossfades over
one frame without clicks, and the latency stays the same
([details](docs/usage.md#switching-stages-and-the-filter)).

**Latency.** About 56 ms from the microphone to the app with a USB microphone: 8 ms microphone,
11 ms for turning PipeWire's blocks into 10 ms frames (both measured live) and 37 ms for the chain.
Most of it is DeepFilterNet3's lookahead. The GNOME app shows the current value.

## Installation

There are no distribution packages yet, install it with Cargo, from crates.io or from source. You
need Rust 1.96 or newer (from [rustup](https://rustup.rs) if your distribution's is older), clang,
the PipeWire development files and, for the GNOME app, GTK 4 and libadwaita. Filtering for all
apps needs WirePlumber 0.5 or later as the session manager (Debian 13 and Ubuntu 24.10 have it,
Ubuntu 24.04 not yet); with an older one Voice Filter is a virtual microphone only:

| Distribution  | Packages                                                  |
| ------------- | --------------------------------------------------------- |
| Arch Linux    | `rust clang pipewire gtk4 libadwaita`                     |
| Debian/Ubuntu | `clang libpipewire-0.3-dev libgtk-4-dev libadwaita-1-dev` |

```bash
# from crates.io, into ~/.cargo/bin
cargo install --locked voice-filter
# the same without the GNOME app, no GTK needed
cargo install --locked voice-filter --no-default-features
# from a clone of the repository
git clone https://github.com/chpio/voice-filter.git && cd voice-filter
cargo install --locked --path .
```

Cargo builds for every x86-64 CPU. Built for yours instead
(`RUSTFLAGS="-C target-cpu=native" cargo install --locked voice-filter`), it took 6 % less time
per frame on an AMD Zen 4 ([measurements](docs/measurements.md#target-cpu)); such a binary only
runs on CPUs with the same instruction set extensions.

## Usage

```bash
# lists the microphones, the chosen one marked with *
voice-filter inputs
# lists the output devices (speakers), the same way
voice-filter outputs
# filters the default input for all apps, until Ctrl+C
voice-filter run --output <name>
# filters the microphone <name> for all apps that record from it
voice-filter run --input <name>
# only for the apps that record from "Voice Filter"
voice-filter run --mode manual
# the same, printing the latency and the dropouts every 10 s
voice-filter run --verbose
# opens the GNOME app for a running `voice-filter run`
voice-filter ui
```

1. Start `voice-filter run`. Every app that records from the default input gets it filtered,
   nothing to set up in the apps; `--input` filters a particular microphone instead, `--output`
   names the speakers whose echo gets removed.
2. Open `voice-filter ui` to choose the microphone and the speakers, switch stages on and off or
   adjust their settings. `voice-filter install-desktop` adds the app to the desktop, and
   `voice-filter run` shows an icon in the tray that switches the filter and opens the app.

All of it is kept for the next runs. The microphone is only in use while an app records, and only
then the chain runs. Without WirePlumber 0.5 or later, and with `--mode manual`, only the apps
that record from the microphone **Voice Filter** get the filter.

More in the docs:

- [docs/usage.md](docs/usage.md): the two modes, what's kept between runs, the GNOME app and the
  tray, and the offline mode (`voice-filter process in.wav out.wav`) to try settings on WAV files
- [docs/dbus.md](docs/dbus.md): the D-Bus interface the app uses, to control the filter from
  scripts

### Tips

- Leave the volume of **Voice Filter** in the system's sound settings at **100 %**: the chain
  already brings the voice to its level and keeps the peaks below −1 dBFS, a lower volume only
  makes it quieter, a higher one can clip.
- Turn off the app's own noise suppression, echo cancellation and automatic gain control where it
  has them (in Discord e.g. Krisp, echo cancellation and automatic gain control): Voice Filter
  does that already, twice works against each other.
- Get **close to the microphone**, about a hand's width. The louder the voice reaches it against
  the room, the speakers and the noise, the better every stage works; raising the microphone's
  volume above 100 % in software doesn't help, the automatic gain control does that anyway.
- With **headphones** there's no echo, switch the echo cancellation off
  ([over speakers](docs/usage.md#echo-cancellation-over-speakers)).

## Development

```bash
cargo test --workspace
# the model runs about 10x faster in release builds
cargo test --release -p deep-filter-rt
cargo clippy --workspace --all-targets
# processing time and allocations of every stage per frame
cargo bench --bench stages [-- in.wav]
# level after every stage and the gains over time, as TSV, e.g. to try settings
cargo run --release --example trace -- in.wav \
  [--set expander.threshold=6] [--off echo] > trace.tsv
# every WAV of a directory through the chain or some stages, e.g. to score them on a test set
cargo run --release --example batch -- in/ out/ [--only denoise] [--set denoise.level=-30]
.venv/bin/python scripts/score.py clean/ noisy/ out/
```

How the defaults of the noise suppression were measured, and how to repeat it, is in
[docs/measurements.md](docs/measurements.md). The design, the choice of models, the latency
measurements and what's still open are in [docs/concept.md](docs/concept.md). The DeepFilterNet3
inference is vendored as its own crate, [deep-filter-rt](crates/deep-filter-rt), because
upstream's crate on crates.io is outdated.

## Repository

The source lives on [GitHub](https://github.com/chpio/voice-filter), with a mirror on
[GitLab](https://gitlab.com/chpio/voice-filter).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

The noise suppression is [DeepFilterNet](https://github.com/Rikorose/DeepFilterNet) by Hendrik
Schröter (MIT or Apache-2.0), see [crates/deep-filter-rt/NOTICE.md](crates/deep-filter-rt/NOTICE.md).
The echo cancellation is WebRTC's AEC3 as ported to Rust by
[sonora](https://github.com/dignifiedquire/sonora) (BSD-3-Clause).
