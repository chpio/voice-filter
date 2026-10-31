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

- [ ] Before AEC3 from [sonora](https://github.com/dignifiedquire/sonora) goes into the audio
  path: check that it no longer allocates per frame. sonora 0.2.0 makes about 40 allocations per
  10 ms frame (counted with a counting allocator in the spike); the fix is the open
  [PR #40](https://github.com/dignifiedquire/sonora/pull/40) (on #39). If it doesn't get merged,
  vendor sonora with it, like `deep-filter-rt`.
- [ ] Flush denormals around the calls into sonora (FTZ and DAZ in MXCSR on x86_64), as upstream
  WebRTC does: [issue #34](https://github.com/dignifiedquire/sonora/issues/34) reports frames 10–50×
  slower while the reference is silent ([PR #38](https://github.com/dignifiedquire/sonora/pull/38)
  is open). Not seen in the spike (mean 60 µs, at most 0.6 ms per frame, the same output either
  way), but cheap insurance.
- [ ] A second test recording with only speech from the speakers, like the other side of a call:
  in the first one DeepFilterNet3 alone already removed the echo, while it let speech from a phone
  through in an earlier test.

## Releases

- [ ] Before the first release: check the release PR title `chore: bump version (v{{ version }})` in
  `release-plz.toml` with two packages. If one PR releases voice-filter and deep-filter-rt with
  different versions, release-plz probably can't fill in `version`.
