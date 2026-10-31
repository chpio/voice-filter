# TODO

Work items that don't change the design; the open design questions and TODOs are in
[docs/concept.md](docs/concept.md). Remove an item once it's done.

## CI

- [ ] A CI image with the PipeWire development files (plus clang for the bindings `pipewire-rs`
  generates) and the GTK 4 and libadwaita ones for `crates/voice-filter-gtk`: without them the CI
  can't build the workspace, everything is checked locally only so far.
- [ ] Check the first CI run with `crates/deep-filter-rt`: `cargo deny` (license allow list, sources),
  `cargo machete` and the MSRV check (1.96) haven't run locally.

## Releases

- [ ] Before the first release: check the release PR title `chore: bump version (v{{ version }})` in
  `release-plz.toml` with two packages. If one PR releases voice-filter and deep-filter-rt with
  different versions, release-plz probably can't fill in `version`.

## Live mode

- [ ] A "Listen" switch in the app: a third stream that plays what "Voice Filter" outputs on the
  default output (headphones, speakers feed back into the microphone), to hear the filter while
  tuning it. Until then: `pw-loopback --capture-props='target.object=voice-filter'`.
