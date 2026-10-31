# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

`voice-filter` is going to be a live voice filter for Linux: a single Rust process acting as a PipeWire node that reads the microphone, runs it through a chain of stages (echo cancellation, DeepFilterNet3 denoising, optional Hush for suppressing other voices, classic DSP voice enhancement) and exposes the result as the virtual microphone "Voice Filter". The design, model choice, latency budget and roadmap are in `docs/concept.md` (started as a German document, now authoritative here). So far there's the vendored DeepFilterNet3 inference (`crates/deep-filter-rt`), the stage chain with DeepFilterNet3 as its only stage and the offline mode; the live PipeWire part doesn't exist yet.

## Commands

```bash
cargo build
cargo clippy --workspace --all-targets
cargo test --workspace
cargo test --release -p deep-filter-rt   # the model runs about 10x faster than in debug builds
cargo run --release -- process in.wav out.wav   # offline mode, prints latency and time per stage
cargo fmt --all            # 4 spaces (rustfmt default, matches .editorconfig)
cargo deny check           # licenses, advisories, sources (deny.toml)
cargo machete              # unused dependencies
typos                      # spell check
```

Tests live in `tests/` of each crate (integration tests against the library, no inline `#[cfg(test)]` modules).

## Architecture

- **Workspace**: the root package `voice-filter` and `crates/deep-filter-rt`; edition, MSRV and license are shared in `[workspace.package]` of the root `Cargo.toml`.
- **Crate layout** of voice-filter: all modules live in the library (`lib.rs`) so tests under `tests/` can use them; the binary (`main.rs`, clap, subcommand `process`) only wires them up.
- **Stages** (`stage.rs`): the trait `Stage` (name, latency in samples, `process` of one `Frame`, `[f32; FRAME_LEN]` in place) runs in the audio path, so no allocations there; `SAMPLE_RATE` (48 kHz) and `FRAME_LEN` (480, 10 ms) are in `lib.rs`. `Chain` (`chain.rs`) runs them in order and sums their latency. `Denoise` (`denoise.rs`) wraps `DfTract` with preallocated `[1, 480]` arrays, its latency is `DfTract::delay` (30 ms).
- **Offline mode** (`offline.rs`): `read_wav` takes 48 kHz only (int or float, channels mixed down to mono), `process` runs the samples frame by frame through the chain, times every stage (mean and max per frame, against the 10 ms a frame may take live) and shifts the output by the chain's latency (padding the input with silence), so it lines up with the input; `write_wav` writes mono 32-bit float. The max times are only meaningful on an idle machine.
- **deep-filter-rt**: DeepFilterNet's `deep_filter` crate (`libDF`, upstream commit `d375b2d`), vendored without its history and reduced to the real-time inference: `lib.rs` (STFT state, ERB features, gains, post filter) and `tract.rs` (`DfParams` loads the model, `DfTract` runs the three ONNX parts encoder, ERB decoder and DF decoder with tract per 10 ms frame). The DeepFilterNet3 model is embedded from the unpacked files in `models/DeepFilterNet3/` (the keys in its `config.ini` belong to the model even where they look like typos, `_typos.toml` excludes the folder). `NOTICE.md` names the origin and lists the changes against upstream (required by Apache-2.0); it's published on its own, so its API is public. Debug builds of tract 0.21 failed on the model with "duplicate name /convt3/Conv.bias" (a debug-only check), 0.23 doesn't.
- The planned architecture (stage trait, 48 kHz frames of 10 ms / 480 samples, fixed 3:1 resampling for Hush, lock-free settings, D-Bus interface for the GNOME app, offline WAV test mode) is described in `docs/concept.md`. Keep this section in sync with the code once it exists.

## Conventions

- Code, comments and docs are in English.
- CI (`.gitea/workflows/ci.yaml`) runs in the prebuilt image `ci-rust`. Two parallel jobs: `check` (nothing compiles) runs `cargo fmt --all --check`, `cargo deny --locked check`, `cargo machete`, `typos` and `actionlint`/`zizmor` on the workflows, each even if one before failed; `test` runs clippy, `cargo doc` with `RUSTDOCFLAGS=-D warnings`, `cargo +<rust-version> check --locked` (the MSRV, read from `rust-version` in `[workspace.package]` of `Cargo.toml`) and the tests, all with `--workspace`. On pushes to `main`, after both, `release` and then `release-pr` run release-plz (see Releases). `.gitea/workflows/advisories.yaml` runs `cargo deny --locked check advisories` daily. Actions are pinned by commit hash (Renovate updates hash and version comment), checkouts don't persist the token, the workflows only get `contents: read`; the release jobs use the secrets `RELEASE_PLZ_TOKEN` (Gitea token with repository read/write) and `CARGO_REGISTRY_TOKEN`, and bypass the image's registry proxy with an empty `CARGO_HOME`. Once `pipewire-rs` is added, the build needs the PipeWire development files, so the jobs that compile need an image with them.
- Conventional commit prefixes (`chore:`, `docs:`, `build:`, `ci:` …). Dependency upgrades are committed in Renovate's style, also by hand: `chore(deps): …` (e.g. `chore(deps): update dependencies`), release-plz only recognizes that. Only `feat:`, `fix:` and `chore(deps):` end up in the changelog and trigger a release (only if they touch a file of the package), so a user-visible change needs one of them, even if it's part of a refactoring (otherwise as a commit of its own); their description is the changelog entry, written for users.
- Releases are done by release-plz (`release-plz.toml`): the `release-pr` job keeps a PR `chore: bump version (vX.Y.Z)` open that bumps the version in `Cargo.toml`/`Cargo.lock` and adds the `CHANGELOG.md` section, generated from the commits, so there's no hand-written `## Unreleased` section. Merging it makes the `release` job publish to crates.io and create the tag `vX.Y.Z` (unsigned, through the Gitea API) and the Gitea release. `deep-filter-rt` is released the same way with its own version, changelog (`crates/deep-filter-rt/CHANGELOG.md`) and tags `deep-filter-rt-vX.Y.Z`, and gets the semver check, voice-filter doesn't (its library isn't a public API). `release-plz update` shows the bump and changelog locally. **Currently disabled** with `release = false` in `release-plz.toml`, so the skeleton isn't published (the `release` job fails with "no public packages found" meanwhile, that's expected); remove it for the first release.
- `use` statements for external crates (including `std`) start with `::` (e.g. `use ::std::sync::Arc;`); `crate::`/`super::` paths stay as they are. rustfmt has no option for this, so keep it by hand.
- No allocations, locks or syscalls in the audio path (per-frame processing); everything it needs is allocated up front.
