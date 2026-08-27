# Issue #077 — Cryptographic Signature-Based Gasless Task Assignment

## Plan

- [x] New `src/gasless.rs` module: `GaslessAssignment` struct, `GaslessKey` storage enum
- [x] `register_signing_key` (contributor) + `admin_set_signing_key` (admin) — Ed25519 pubkey registration
- [x] Per-contributor nonce tracking (`GaslessKey::Nonce`) for replay protection
- [x] `assign_task_gasless(relayer, request, signature)` relayer endpoint
  - [x] domain separation (`request.contract == current contract`)
  - [x] `expiration_ledger` time bound
  - [x] nonce match + consume
  - [x] `env.crypto().ed25519_verify` over canonical XDR payload
- [x] `gasless_assignment_payload` view so off-chain signers sign exactly what's verified
- [x] Extract `apply_task_assignment` helper; reuse from `assign_task` + gasless path
- [x] Events: `emit_signing_key_registered`, `emit_gasless_assignment`
- [x] `src/gasless_test.rs` — 11 unit tests
- [x] `Cargo.toml` dev-dep `ed25519-dalek = "2"`
- [x] README module table row

## Review

- `cargo test --lib gasless` → 11/11 pass.
- `cargo test --lib` → 127 pass / 12 fail; the 12 failures are **pre-existing on
  `main`** (verified via `git stash`): 9 `benchmark::*`, 2 `swap_router_test::test_refund_on_*`,
  1 `test::test_create_and_complete_task_flow`. No new regressions; +11 new passing tests.
- `cargo build --target wasm32-unknown-unknown --release` → clean.
- `cargo clippy --lib --tests` → no new warnings in `gasless.rs` / `gasless_test.rs`.

## Notes / follow-ups

- SECP256k1 support intentionally out of scope (Ed25519 is Stellar-native). It is a
  mechanical add via `env.crypto().secp256k1_recover` behind a key-type discriminator.
- Nonces + signing keys use instance storage, consistent with `Task` storage.
