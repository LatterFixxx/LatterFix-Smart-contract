# Issue #094 — Multi-Signature Role Management Recovery Protocol

## Plan

- [x] New `src/social_recovery.rs`: `RecoveryStatus` / `RecoveryProposal` / `RecoveryConfig` / `RecoveryKey`
- [x] `configure_recovery` (admin-only, threshold >= 1, ttl > 0); defaults 3 / 7 days
- [x] `propose_recovery` — top-tier proposer, auto-approves, one active proposal at a time (stale one lazily expired)
- [x] `approve_recovery` — top-tier guardian, one vote each, TTL-checked, Approved on threshold
- [x] `execute_recovery` — Approved-only, TTL-checked, re-count live-tier approvals, snapshot-admin check, rotate `DataKey::Admin`
- [x] `cancel_recovery` — proposer or sitting admin
- [x] `is_eligible_guardian` / `get_active_recovery` / `get_recovery_proposal` views
- [x] `src/lib.rs`: module wiring + thin contract methods + `get_admin` view
- [x] `src/events.rs`: `emit_recovery_proposed/approved/executed/cancelled`
- [x] `src/social_recovery_test.rs` — 24 unit tests
- [x] README module row + feature section

## Review

- `cargo test --lib social_recovery` → 24/24 pass.
- `cargo test --lib` → 140 pass / 12 fail. The 12 are **pre-existing on `main`**
  (`benchmark::*` ×9, `swap_router_test::test_refund_on_*` ×2,
  `test::test_create_and_complete_task_flow`). No regressions; +24 new passing tests.
- `cargo build --target wasm32-unknown-unknown --release` → clean.
- `cargo clippy --lib --tests` → no new warnings in the added files.

## Design notes

- On-chain `require_auth()` approvals (each = a host-verified signature), matching
  the existing `src/multisig.rs` threshold model — chosen over off-chain sig
  collection for consistency and simplicity.
- Eligibility = reputation tier `Master` (>=1000) or `Legend` (>=2500) via
  `reputation::get_user_tier`. Note reputation has two stores; tier gating uses
  `reputation::ReputationKey::UserPoints` (the one `get_user_tier` reads).
- `admin_at_proposal` snapshot + re-check at execution prevents a stale proposal
  from silently clobbering a newer admin.
