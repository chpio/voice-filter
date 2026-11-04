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

- [ ] Report to tract: its depthwise convolution (`tract-linalg` 0.23.9, `x86_64/depthwise.rs`)
  has an AVX kernel only for contiguous input (`in_stride == 1`), everything else runs a scalar
  loop compiled for the baseline, and DeepFilterNet3's convolutions mostly take that one. Built
  with `-C target-feature=+avx2,+fma`, `denoise` took about 10 µs less per frame (135.5 instead of
  145.9 µs, see "Target CPU" in `docs/measurements.md`); `perf` put the difference in
  `depthwise_w_f32` (4285 + 382 samples for its AVX part, 2379) and `DepthwiseDeconv::eval_t_2d`
  (545, 195). Still one float per instruction in both builds, the sum over the taps can't be
  vectorized as written. The fix belongs in tract (the strided case vectorized, or compiled with
  `#[target_feature]` like the contiguous one): `multiversion` on our functions doesn't reach it,
  and a `target-cpu` doesn't apply to `cargo install`.

## Releases

- [ ] The release PR title `chore: bump version (v{{ version }})` in `release-plz.toml` works while
  voice-filter and deep-filter-rt get the same version (0.1.0, 0.1.1); once one PR releases them
  with different versions (e.g. a breaking change of deep-filter-rt), release-plz probably can't
  fill in `version`: check that PR's title.
