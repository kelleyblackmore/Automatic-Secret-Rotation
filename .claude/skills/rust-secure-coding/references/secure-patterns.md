# Secure Rust patterns

## Panics are a denial of service

In services, untrusted input must never reach `unwrap()`, `expect()`, `[i]`
indexing, `/` by a value that can be zero, or `as` casts that can truncate.

```rust
// Bad
let n: u32 = input.parse().unwrap();
let item = items[idx];
let len = big as u16;

// Good
let n: u32 = input.parse().map_err(|_| Error::BadInput)?;
let item = items.get(idx).ok_or(Error::NotFound)?;
let len = u16::try_from(big).map_err(|_| Error::TooLarge)?;
```

Clippy lints that catch these: `unwrap_used`, `expect_used`, `indexing_slicing`,
`cast_possible_truncation`, `arithmetic_side_effects` (see `assets/Cargo-lints.toml`).
Use `checked_*`/`saturating_*` arithmetic on sizes and counts from input.

## Commands

No Clippy lint catches a shell-built command, so check for this by eye in review.

```rust
// Bad: shell, string-built
Command::new("sh").arg("-c").arg(format!("git log {branch}")).status()?;

// Good: fixed program, argument list, --end-of-options before user values
Command::new("git").args(["log", "--end-of-options", branch, "--"]).status()?;
```

## SQL

```rust
// Bad
sqlx::query(&format!("SELECT id FROM users WHERE email = '{email}'"))

// Good: bound parameters (sqlx::query! also checks the query at compile time)
sqlx::query("SELECT id FROM users WHERE email = $1").bind(email)
```

## Paths

```rust
// Bad: "../" in name escapes the directory
std::fs::read(base.join(name))?;

// Good
let base = base.canonicalize()?;
let target = base.join(name).canonicalize()?;
if !target.starts_with(&base) {
    return Err(Error::Forbidden);
}
```

Archives: reject entries with absolute paths or `..` components before extracting.

## Unbounded input

Cap sizes before allocating or reading: `reader.take(MAX).read_to_end(&mut buf)`,
request body limits in your framework (axum's `DefaultBodyLimit`), and
`serde_json` with a size-checked input. Never `Vec::with_capacity(len_from_input)`
without a bound.

## Deserialization

`serde` is safe from code execution, but validate what you get: use
`#[serde(deny_unknown_fields)]` on external input and newtypes that validate
in `TryFrom`. Don't deserialize untrusted data with `bincode`/`postcard`
without a size limit.

## Crypto, randomness and secrets

- Use RustCrypto or `ring`/`aws-lc-rs`; never hand-roll crypto.
- Randomness for secrets: `rand::rngs::OsRng` / `getrandom`, not a seeded RNG.
- Passwords: `argon2` crate. Compare secrets with `subtle::ConstantTimeEq`.
- Wrap secrets in `secrecy::SecretString` so they don't end up in `Debug`
  output or logs, and zeroize on drop.

## TLS

Use `rustls` (or the platform via `native-tls`). Never install a
`ServerCertVerifier` that accepts everything; don't set
`danger_accept_invalid_certs(true)` on `reqwest`.

## HTTP clients

```rust
let client = reqwest::Client::builder()
    .timeout(Duration::from_secs(10))
    .https_only(true)
    .build()?;
```

## unsafe

- Prefer a safe abstraction from a well-reviewed crate.
- Every `unsafe` block: `// SAFETY:` explaining why the invariants hold.
- Enable `unsafe_op_in_unsafe_fn` and `clippy::undocumented_unsafe_blocks`.
- Run Miri on code with `unsafe`.
- Add `#![forbid(unsafe_code)]` to crates that don't need it.

## Logging and errors

Use `tracing`; don't log secrets or whole request bodies. Return generic errors
to clients (`thiserror` for typed errors internally, `anyhow` in binaries).
