# TODO

Work items that don't change the design; the open design questions and TODOs are in
[docs/concept.md](docs/concept.md). Remove an item once it's done.

## CI

- [ ] A CI image with the PipeWire development files (plus clang for the bindings `pipewire-rs`
  generates), needed before `pipewire-rs` becomes a dependency of voice-filter. Until then the
  PipeWire part is developed and checked locally only.
- [ ] Check the first CI run with `crates/deep-filter-rt`: `cargo deny` (license allow list, sources),
  `cargo machete` and the MSRV check (1.96) haven't run locally.

## Releases

- [ ] Before the first release: check the release PR title `chore: bump version (v{{ version }})` in
  `release-plz.toml` with two packages. If one PR releases voice-filter and deep-filter-rt with
  different versions, release-plz probably can't fill in `version`.

## Live mode

- [ ] Refuse to start a second instance with a clear message: two running instances record from each
  other's "Voice Filter" and keep each other running.
