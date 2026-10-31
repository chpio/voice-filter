# TODO

Work items that don't change the design; the open design questions and TODOs are in
[docs/concept.md](docs/concept.md). Remove an item once it's done.

## CI

- [ ] A CI image with the PipeWire development files (plus clang for the bindings `pipewire-rs`
  generates) and the GTK 4 and libadwaita ones for the feature `gui`: without them the CI
  can't build the workspace, everything is checked locally only so far.
- [ ] Check the first CI run with `crates/deep-filter-rt`: `cargo deny` (license allow list, sources),
  `cargo machete` and the MSRV check (1.96) haven't run locally.

## Echo cancellation

- [ ] Remove `[patch.crates-io]` for sonora from `Cargo.toml` (and its `allow-git` in `deny.toml`)
  once a release of sonora has [PR #40](https://github.com/dignifiedquire/sonora/pull/40), its
  allocation-free echo canceller; `tests/echo.rs` checks there are no allocations per frame.
- [ ] A second test recording with only speech from the speakers, like the other side of a call,
  through `process --reference`: in the first one DeepFilterNet3 alone already removed the echo,
  while it let speech from a phone through in an earlier test.
- [ ] Try it live: the reference stream from the monitor and the microphone run on different
  clocks, AEC3 keeps estimating the delay; check it keeps up over a longer call.

## Releases

- [ ] Before the first release: check the release PR title `chore: bump version (v{{ version }})` in
  `release-plz.toml` with two packages. If one PR releases voice-filter and deep-filter-rt with
  different versions, release-plz probably can't fill in `version`.
