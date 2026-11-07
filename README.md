<img src="data/net.b128.VoiceFilter.svg" alt="" width="96" align="right">

# Voice Filter

**A live voice filter for Linux.** Voice Filter shows up in PipeWire as a virtual microphone named
_Voice Filter_. Any app (Discord, the browser, OBS) can use it like a regular microphone and gets
your voice without the echo of your speakers and the background noise, with a clearer, even tone.

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
> enhancement work. What's still open is in the [roadmap](docs/concept.md#roadmap).

## How it works

A single process reads the microphone, runs it through a chain of stages in 10 ms frames and
plays the result into the virtual microphone:

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

Every stage can be switched off while it runs. A stage that is off is skipped (it uses no CPU) and
its input plays instead, delayed by the stage's latency, so the total latency never changes.
Switching crossfades over one frame, so there are no clicks. The echo cancellation takes a few
seconds after being switched on until it removes the echo fully again, as it missed what the
speakers played meanwhile. The whole filter switches off the same way, all stages at once, while
each stage keeps its own switch for when the filter is on again; kept for the next runs too.
Switched off, it still delays the microphone by its latency (37 ms, 30 ms of it the noise
suppression, see below): dropping the delay would cut a piece out of the sound when switching off
and insert a gap when switching on, heard as a click or a stutter in a call, while at the same
latency switching just crossfades.

**Latency.** About 56 ms from the microphone to the app with a USB microphone: 8 ms microphone,
11 ms for turning PipeWire's blocks into 10 ms frames (both measured live) and 37 ms for the chain.
Most of it is DeepFilterNet3's lookahead. The GNOME app shows the current value.

## Installation

There are no distribution packages yet, install it with Cargo, from crates.io or from source. You
need Rust 1.96 or newer (from [rustup](https://rustup.rs) if your distribution's is older), clang,
the PipeWire development files and, for the GNOME app, GTK 4 and libadwaita:

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
git clone https://gitlab.com/chpio/voice-filter.git && cd voice-filter
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
# provides "Voice Filter", until Ctrl+C
voice-filter run --input <name> --output <name>
# the same, printing the latency and the dropouts every 10 s
voice-filter run --verbose
# opens the GNOME app for a running `voice-filter run`
voice-filter ui
```

1. Start `voice-filter run`. Without `--input` it reads the default input. A microphone given with
   `--input` is kept for the next runs. `--output` names the speakers whose echo gets removed,
   kept the same way; without one it's the default output.
2. Select **Voice Filter** as the microphone in your app (or as the default input of the system).
3. Open `voice-filter ui` to choose the microphone and the speakers, switch stages on and off or
   adjust their settings. All of it is kept for the next runs, in `~/.local/state/voice-filter/`
   (`$XDG_STATE_HOME`); `chain` there lists what differs from the defaults, one setting per line
   (`denoise off`, `denoise.limit 30`), delete it to go back to them.

The microphone is only in use while an app records from Voice Filter, and only then the chain
runs: while no app records, `voice-filter run` takes no CPU. If Voice Filter is the
default input, set the microphone with `--input` or in the app; otherwise it falls back to the
source with the highest priority.

Only one `voice-filter run` can be active at a time. A second one stops with "Voice Filter is
already running".

Voice Filter stays after `voice-filter run` stopped, until PipeWire restarts (e.g. at the next
login): apps that record from it stay with it and get silence until `voice-filter run` is back,
rather than switching to the unfiltered microphone. To remove it, stop `voice-filter run`, look up
the ID of Voice Filter under _Sources_ in `wpctl status` and run `pw-cli destroy <id>` (not
`pw-cli destroy voice-filter`: that name also matches the client of `voice-filter run`).
`voice-filter run` exits with an error when it loses its connection to PipeWire, so a service
manager can start it again.

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
- With **headphones** there's no echo, switch the echo cancellation off. Over speakers it removes
  the echo; what's left of it it turns down band by band, and the voice with it while you talk
  into the playback, less while it hears you talk (tuned for that). **Voice Protection** (off,
  0 %) keeps a part of your voice as it was while you talk and leaves the rest of the echo to the
  noise suppression, which removes music by itself; while you're silent the echo cancellation acts
  fully. Higher keeps more of the voice but lets more of the playback through under it.

### The GNOME app

`voice-filter ui` controls a running `voice-filter run`. It's a separate process, so closing it
never interrupts the audio. `voice-filter install-desktop` adds it to the apps of the desktop, with
its icon (it writes `~/.local/share/applications/net.b128.VoiceFilter.desktop`,
`~/.local/share/icons/hicolor/scalable/apps/net.b128.VoiceFilter.svg` and the two icons of the tray
in `~/.local/share/icons/hicolor/symbolic/apps/`; delete them to remove it). In the app you can:

- switch the whole **filter** off and on: off, apps get the microphone as it is, still at the
  same latency (so switching doesn't click or stutter), and every stage keeps its own switch and
  settings
- choose the **microphone**, switched while the filter keeps running
- choose the **speakers** whose echo the echo cancellation removes (in its row); with headphones
  there's no echo, it can be off. Its status shows whether it hears the speakers, how loud, and the
  delay of their echo
- turn on **Listen** to hear yourself through the filter on the default output (use headphones),
  at a volume of its own; **Keep Listening** turns it on again after a restart, e.g. to keep
  listening quietly (the microphone stays in use then)
- switch **each stage** on and off and adjust its **settings**, e.g. the limit of the noise
  suppression (lower values keep more of the original voice, 100 dB removes all the noise it can)
  or the frequency of the high-pass; a stage's settings go back to their defaults with its
  **Reset** button, everything with **Reset the Filter…** in the main menu
- watch the **levels** of the microphone and of Voice Filter, and the background noise of the
  microphone: above −45 dBFS the app points out that it's too noisy
- watch the **latency**, the processing time per frame (mean, ~95 %, ~99 % and the longest, of
  all stages, the echo cancellation and the noise suppression on their own and the other stages
  together) and dropouts

`voice-filter run` also shows an icon in the tray: a click opens the app, its menu switches the
filter, the echo cancellation (no echo with headphones) and Listen, and the icon shows whether it's
on (the microphone between noise and a clean wave, as on the app icon, struck through while it's
off). KDE and most other desktops show it as they are; GNOME Shell needs the extension “AppIndicator
and KStatusNotifierItem Support” (on Arch `gnome-shell-extension-appindicator`).

The app and the icon in the tray speak the language of the desktop (`LANG`, `LANGUAGE`), English
where there's no translation; translations are in `i18n/`, one Fluent file per language.

The info button at the end of a row explains what the setting or the status value means; an
expanding row (a stage, the processing time) explains itself as the first line inside. The status
values (latency, processing time, dropouts, the echo cancellation's status and the background noise)
have a copy button and can be selected, e.g. for a bug report.

### Offline mode

```bash
voice-filter process in.wav out.wav
# with what the speakers played, for the echo cancellation
voice-filter process in.wav out.wav --reference played.wav
```

Runs a WAV file (48 kHz, mixed down to mono) through the same chain and writes the result, lined
up with the input. It prints the latency and the processing time of every stage. Use it to compare
settings, e.g. `--atten-lim 12` for a limit of 12 dB; it takes the defaults, not the settings kept
for `run` (`run --atten-lim` sets the limit and keeps it, like the app). For the echo
cancellation, record the microphone and the monitor of the speakers at the same time, e.g.:

```bash
pw-record --target <microphone> --rate 48000 --channels 1 in.wav &
pw-record --target <speakers> -P '{ stream.capture.sink = true }' \
    --rate 48000 --channels 1 played.wav &
```

### D-Bus

The daemon serves `net.b128.VoiceFilter.Daemon` on the session bus (object `/net/b128/VoiceFilter`,
interface `net.b128.VoiceFilter1`), which the GNOME app uses too:

| Member            | Type                 | Description                                                                                                         |
| ----------------- | -------------------- | ------------------------------------------------------------------------------------------------------------------- |
| `Input`           | `s`, read/write      | the microphone's `node.name`, empty for the default input                                                           |
| `Output`          | `s`, read/write      | the speakers' `node.name` (for the echo), empty for the default output                                              |
| `Listen`          | `b`, read/write      | plays Voice Filter on the default output                                                                            |
| `KeepListen`      | `b`, read/write      | keeps `Listen` for the next runs (off by default)                                                                   |
| `ListenVolume`    | `d`, read/write      | the volume of `Listen` in dB, −40 to 0, kept for the next runs                                                      |
| `Enabled`         | `b`, read/write      | the whole filter on or off (off, the microphone passes as it is), kept for the next runs                            |
| `SetStageEnabled` | method `(sb)`        | switches a stage on or off by name, kept for the next runs                                                          |
| `Parameters`      | `a(ssssddddd)`, read | the settings: stage, name, title, unit, min, max, step, default, value                                              |
| `SetParameter`    | method `(ssd)`       | sets a setting of a stage (stage, name, value), within its range, kept for the next runs                            |
| `Stages`          | `a(sb)`, read        | the stages in order and whether they're on                                                                          |
| `Inputs`          | `a(ssi)`, read       | the microphones: name, description, priority                                                                        |
| `Outputs`         | `a(ssi)`, read       | the output devices, the same way                                                                                    |
| `Echo`            | `a{sd}`, read        | the echo cancellation: frame counters, level of the speakers, AEC3's ERLE/ERL (of its linear filter only) and delay |
| `Latency`         | `a{sd}`, read        | `total`, `microphone`, `bridge`, `chain` and `app`, in ms                                                           |
| `Quantum`         | `t`, read            | samples per PipeWire cycle                                                                                          |
| `MaxFrameTime`    | `d`, read            | longest processing time of a frame over the last 10 s, in ms                                                        |
| `FrameTimes`      | `a(sdddd)`, read     | processing time per frame over the last 10 s in ms: part, mean, 95 % and 99 % percentile, longest                   |
| `Levels`          | `a{sd}`, read        | `input` and `output` (dBFS, falling 20 dB/s) and `input-floor`, the noise floor of the microphone                   |
| `Counters`        | `a{st}`, read        | `frames`, `silence`, `dropped` and `errors` since the start                                                         |

For example with `busctl`:

```bash
vf="net.b128.VoiceFilter.Daemon /net/b128/VoiceFilter net.b128.VoiceFilter1"
busctl --user introspect $vf
busctl --user get-property $vf Parameters
busctl --user call $vf SetParameter ssd denoise limit 12
busctl --user call $vf SetStageEnabled sb agc false
busctl --user set-property $vf Enabled b false
```

The interface may still change before 1.0.

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
[docs/measurements.md](docs/measurements.md).

The design, the choice of models, the latency measurements and the roadmap are in
[docs/concept.md](docs/concept.md). The DeepFilterNet3 inference is vendored as its own crate,
[deep-filter-rt](crates/deep-filter-rt), because upstream's crate on crates.io is outdated.

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
