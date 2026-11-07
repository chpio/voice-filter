# TODO

Work items that don't change the design; the open design questions and TODOs are in
[docs/concept.md](docs/concept.md). Remove an item once it's done.

## Echo cancellation

- [ ] A second test recording with only speech from the speakers, like the other side of a call,
  through `process --reference`: in the first one DeepFilterNet3 alone already removed the echo,
  while it let speech from a phone through in an earlier test.
- [ ] Over a longer call: the reference stream and the microphone run on different clocks, check
  that AEC3 keeps up with the drift.
- [ ] Less choppy speech when talking into the playback: Voice Protection keeps part of the voice
  while you talk (see the concept, "Double talk"). AEC3's own configuration, measured offline
  ("AEC3 tuning in double talk" in `docs/measurements.md`): tuned, AEC3 alone keeps more of the
  voice than the chain today and lets less through besides it; still to decide whether it replaces
  Voice Protection, and to listen to live.

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
