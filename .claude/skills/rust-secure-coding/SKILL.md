---
name: rust-secure-coding
description: Secure coding rules for Rust projects. Use whenever writing, reviewing, or changing Rust code, adding or upgrading crates, or setting up Cargo.toml, rust-toolchain.toml, a Dockerfile, or CI for a Rust project. Requires the latest stable Rust toolchain, the latest crate versions, rustfmt and clippy (with -D warnings) passing, cargo-audit and cargo-deny clean, minimal unsafe, and secure Rust patterns.
---

# Rust Secure Coding

Memory safety is the default in Rust; most remaining risk is in `unsafe`,
dependencies, panics, and the usual injection and validation bugs.

## Hard rules

1. **Latest stable Rust**, pinned in `rust-toolchain.toml`.
2. **Latest crates.** Every crate you add or touch is at its latest release.
   Never write a version from memory.
3. **`cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` pass.**
4. **`cargo audit` and `cargo deny check` are clean.**
5. **No new `unsafe`** unless there's no safe alternative; every `unsafe` block
   has a `// SAFETY:` comment.
6. **Secure patterns.** Follow `references/secure-patterns.md`.

## 1. Toolchain

```bash
git ls-remote --tags https://github.com/rust-lang/rust | grep -oE '1\.[0-9]+\.[0-9]+$' | sort -V | tail -1
rustup update stable
```

`rust-toolchain.toml` (see `assets/rust-toolchain.toml`):

```toml
[toolchain]
channel = "1.99.1"
components = ["clippy", "rustfmt"]
```

| File | Setting |
|---|---|
| `rust-toolchain.toml` | `channel = "<latest stable>"` |
| `Cargo.toml` | `rust-version = "<latest minor>"`, `edition = "2024"` |
| `Dockerfile` | `FROM rust:<version>-slim@sha256:…` (build stage) |
| CI | uses `rust-toolchain.toml` (rustup reads it automatically) |

## 2. Crates

```bash
cargo add some-crate             # adds the latest compatible version
cargo search some-crate --limit 1
cargo update                     # latest semver-compatible versions
cargo install cargo-edit && cargo upgrade --incompatible   # bump major versions
```

- Commit `Cargo.lock` (for binaries and libraries alike; CI uses `--locked`).
- Check a new crate is the real one (name, owners, downloads, repository) and
  maintained. Prefer `std` and well-known crates.
- Disable default features you don't need (`default-features = false`).
- Fix code for breaking changes in majors; hold one back only with a comment
  naming the blocker. Commit upgrades separately (`build: upgrade crates`).

## 3. Lint

Use the `[lints]` table in `assets/Cargo-lints.toml`. In a workspace, put it
under `[workspace.lints]` in the root and add `[lints] workspace = true` to
every member crate; members don't inherit it otherwise. Then:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features --locked -- -D warnings
```

Suppress a lint only on the item, with a reason (Rust 1.81+):
`#[expect(clippy::cast_possible_truncation, reason = "len checked < u32::MAX above")]`.
Never `#![allow(clippy::all)]` or a crate-wide `allow` to get green.

## 4. Supply chain

```bash
cargo install cargo-audit cargo-deny --locked
cargo audit                      # RustSec advisories in Cargo.lock
cargo deny check                 # advisories, licenses, banned and duplicate crates, sources
```

Use `assets/deny.toml` as a starting point. Fix advisories by upgrading. An
`ignore` entry needs the advisory ID, a reason, and should be temporary.

## 5. Tests

```bash
cargo test --all-features --locked
```

Fuzz parsers and anything handling untrusted bytes with `cargo fuzz`. Run
`cargo +nightly miri test` on crates that contain `unsafe`.

## 6. Release builds

Set in `Cargo.toml`:

```toml
[profile.release]
overflow-checks = true     # integer overflow panics instead of wrapping
panic = "abort"            # for services where unwinding isn't needed
```

## Checklist

- [ ] Latest stable Rust in `rust-toolchain.toml`, `rust-version`, Dockerfile and CI
- [ ] Added/touched crates at latest; `Cargo.lock` committed
- [ ] `cargo fmt --check` and `cargo clippy … -D warnings` clean; any `expect` has a reason
- [ ] `cargo audit` and `cargo deny check` clean
- [ ] No new `unsafe`, or each block has a `// SAFETY:` comment
- [ ] Code follows `references/secure-patterns.md`
