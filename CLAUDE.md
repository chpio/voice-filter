# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

`voice-filter` is going to be a live voice filter for Linux: a single Rust process acting as a PipeWire node that reads the microphone, runs it through a chain of stages (echo cancellation, DeepFilterNet3 denoising, optional Hush for suppressing other voices, classic DSP voice enhancement) and exposes the result as the virtual microphone "Voice Filter". The design, model choice, latency budget and roadmap are in `docs/konzept.md` (German; a copy of a document whose original is authoritative, see its header). So far there's only the project skeleton, no code.

## Commands

```bash
cargo build
cargo clippy --all-targets
cargo test
cargo fmt                  # 4 spaces (rustfmt default, matches .editorconfig)
cargo deny check           # licenses, advisories, sources (deny.toml)
cargo machete              # unused dependencies
typos                      # spell check (_typos.toml, skips the German concept)
```

Tests live in `tests/` (integration tests against the library, no inline `#[cfg(test)]` modules).

## Architecture

- **Crate layout**: all modules live in the library (`lib.rs`) so tests under `tests/` can use them; the binary (`main.rs`) only wires them up.
- The planned architecture (stage trait, 48 kHz frames of 10 ms / 480 samples, fixed 3:1 resampling for Hush, lock-free settings, control socket, offline WAV test mode) is described in `docs/konzept.md`. Keep this section in sync with the code once it exists.

## Conventions

- Code, comments, README and this file are in English; `docs/konzept.md` stays German.
- CI (`.gitea/workflows/ci.yaml`) runs in the prebuilt image `ci-rust`. Two parallel jobs: `check` (nothing compiles) runs `cargo fmt --check`, `cargo deny --locked check`, `cargo machete`, `typos` and `actionlint`/`zizmor` on the workflows, each even if one before failed; `test` runs clippy, `cargo doc` with `RUSTDOCFLAGS=-D warnings`, `cargo +<rust-version> check --locked` (the MSRV, read from `rust-version` in `Cargo.toml`) and the tests. On pushes to `main`, after both, `release` and then `release-pr` run release-plz (see Releases). `.gitea/workflows/advisories.yaml` runs `cargo deny --locked check advisories` daily. Actions are pinned by commit hash (Renovate updates hash and version comment), checkouts don't persist the token, the workflows only get `contents: read`; the release jobs use the secrets `RELEASE_PLZ_TOKEN` (Gitea token with repository read/write) and `CARGO_REGISTRY_TOKEN`, and bypass the image's registry proxy with an empty `CARGO_HOME`. Once `pipewire-rs` is added, the build needs the PipeWire development files, so the jobs that compile need an image with them.
- Conventional commit prefixes (`chore:`, `docs:`, `build:`, `ci:` …). Dependency upgrades are committed in Renovate's style, also by hand: `chore(deps): …` (e.g. `chore(deps): update dependencies`), release-plz only recognizes that. Only `feat:`, `fix:` and `chore(deps):` end up in the changelog and trigger a release (only if they touch a file of the package), so a user-visible change needs one of them, even if it's part of a refactoring (otherwise as a commit of its own); their description is the changelog entry, written for users.
- Releases are done by release-plz (`release-plz.toml`): the `release-pr` job keeps a PR `chore: bump version (vX.Y.Z)` open that bumps the version in `Cargo.toml`/`Cargo.lock` and adds the `CHANGELOG.md` section, generated from the commits, so there's no hand-written `## Unreleased` section. Merging it makes the `release` job publish to crates.io and create the tag `vX.Y.Z` (unsigned, through the Gitea API) and the Gitea release. `release-plz update` shows the bump and changelog locally. **Currently disabled** with `release = false` in `release-plz.toml`, so the skeleton isn't published; remove it for the first release.
- `use` statements for external crates (including `std`) start with `::` (e.g. `use ::std::sync::Arc;`); `crate::`/`super::` paths stay as they are. rustfmt has no option for this, so keep it by hand.
- No allocations, locks or syscalls in the audio path (per-frame processing); everything it needs is allocated up front.
