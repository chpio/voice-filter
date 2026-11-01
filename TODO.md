# TODO

Work items that don't change the design; the open design questions and TODOs are in
[docs/concept.md](docs/concept.md). Remove an item once it's done.

## CI

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
- [ ] Less choppy speech when talking into the playback: Voice Protection keeps part of the voice
  while you talk (see the concept, "Double talk"). Still to try: AEC3's own configuration for it
  (its suppressor while the near end dominates), once sonora's builder takes one
  ([PR #41](https://github.com/dignifiedquire/sonora/pull/41)).
- [ ] Keep the echo stage running while it's switched off (0.04 ms per frame): it pauses like the
  others, misses what the speakers play and takes about 4 s to remove the echo fully after being
  switched on again. Needs a way for a stage to opt out of pausing.

## Audio path

- [ ] `deep-filter-rt` allocates per frame, about 820 times (`cargo bench --bench stages`, on the
  speech-like signal): almost all in tract running the model, `DfTract`'s own code keeps its STFT
  buffers and rolling spectra (none at a limit of 0 dB, where the model doesn't run). Against the
  rule of no allocations in the audio path; it needs a way to run tract's model on preallocated
  tensors (it already runs on states kept between frames, `TypedSimpleState`).

## Releases

- [ ] Before the first release: check the release PR title `chore: bump version (v{{ version }})` in
  `release-plz.toml` with two packages. If one PR releases voice-filter and deep-filter-rt with
  different versions, release-plz probably can't fill in `version`.
