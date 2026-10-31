# voice-filter

A live voice filter for Linux. It shows up in PipeWire as a virtual microphone named
_Voice Filter_, so any app (Discord, the browser, OBS) can use it like a regular microphone.

Planned:

- removes echo and background noise (_DeepFilterNet3_)
- optionally suppresses other voices (_Hush_)
- enhances the voice with classic DSP: high-pass, EQ, de-esser, compressor, AGC and limiter
- runs entirely on the CPU, with a total latency below 40 ms
- stages and modes can be switched at runtime without clicks, from a GNOME app

The project is at an early stage: so far it only removes noise. The design is described in
[docs/concept.md](docs/concept.md).

## Usage

```bash
voice-filter inputs                   # lists the microphones
voice-filter run --input <name>       # provides "Voice Filter", until Ctrl+C
voice-filter process in.wav out.wav   # filters a 48 kHz WAV file, prints latency and timing
```

Select "Voice Filter" as the microphone in the app, or as the default input of the system. The
microphone given with `--input` is kept for the next runs; without one so far, it's the default
input, which doesn't work once "Voice Filter" is the default itself. The microphone is only in use
while an app records from "Voice Filter".

While it runs, the GNOME app (`voice-filter-gtk`) switches its stages, sets the attenuation limit
and shows its latency. Or over D-Bus, e.g. with `busctl`:

```bash
busctl --user introspect net.b128.VoiceFilter.Daemon /net/b128/VoiceFilter net.b128.VoiceFilter1
busctl --user set-property net.b128.VoiceFilter.Daemon /net/b128/VoiceFilter net.b128.VoiceFilter1 AttenuationLimit d 12
busctl --user call net.b128.VoiceFilter.Daemon /net/b128/VoiceFilter net.b128.VoiceFilter1 SetStageEnabled sb agc false
```

## Building

Needs Rust, the PipeWire development files (`pipewire` on Arch, `libpipewire-0.3-dev` on
Debian/Ubuntu), clang, and for the GNOME app GTK 4 and libadwaita (`gtk4` and `libadwaita` on Arch,
`libgtk-4-dev` and `libadwaita-1-dev` on Debian/Ubuntu).

```bash
cargo build --release
```

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.
