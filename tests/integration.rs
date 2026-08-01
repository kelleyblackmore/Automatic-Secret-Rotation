//! Test-target entry point for the live-service integration suite.
//!
//! Cargo only auto-discovers `tests/*.rs` as test targets — files nested under
//! `tests/integration/` are not compiled unless something declares them. Without
//! this file the suite silently never ran: `cargo test --features integration --
//! --ignored` reported a green "0 passed; 0 failed" instead of exercising Vault
//! and PostgreSQL.
//!
//! These tests need live services and are all `#[ignore]`d, so a normal
//! `cargo test` skips them. To run them:
//!
//!   docker compose -f tests/integration/docker-compose.yml up -d
//!   cargo test --features integration -- --ignored

mod integration {
    mod postgres_tests;
    mod vault_tests;
}
