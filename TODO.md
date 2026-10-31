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
- [ ] Over a longer call: the reference stream and the microphone run on different clocks, check
  that AEC3 keeps up with the drift.
- [ ] Less choppy speech when talking into the playback: AEC3's suppressor takes the voice down with
  the echo. Once sonora's builder takes an AEC3 configuration
  ([PR #41](https://github.com/dignifiedquire/sonora/pull/41)), patch to it, record talking into a
  video (microphone and monitor at once) and compare settings offline.
- [ ] Keep the echo stage running while it's switched off (0.04 ms per frame): it pauses like the
  others, misses what the speakers play and takes about 4 s to remove the echo fully after being
  switched on again. Needs a way for a stage to opt out of pausing.

## Audio path

- [ ] `deep-filter-rt` allocates per frame: `DfTract::process` clones its spectrum tensors into
  the rolling buffers (`spec_buf.clone()`), and tract may allocate when running the model. Against
  the rule of no allocations in the audio path; count them like `tests/echo.rs` does and remove
  them.

## Releases

- [ ] Before the first release: check the release PR title `chore: bump version (v{{ version }})` in
  `release-plz.toml` with two packages. If one PR releases voice-filter and deep-filter-rt with
  different versions, release-plz probably can't fill in `version`.
