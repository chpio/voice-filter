# voice-filter

A live voice filter for Linux. It shows up in PipeWire as a virtual microphone named
_Voice Filter_, so any app (Discord, the browser, OBS) can use it like a regular microphone.

Planned:

- removes echo and background noise (_DeepFilterNet3_)
- optionally suppresses other voices (_Hush_)
- enhances the voice with classic DSP: high-pass, EQ, de-esser, compressor, AGC and limiter
- runs entirely on the CPU, with a total latency below 40 ms
- stages and modes can be switched at runtime without clicks

The project is at an early stage, nothing of this works yet. The design is described (in
German) in [docs/konzept.md](docs/konzept.md).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.
