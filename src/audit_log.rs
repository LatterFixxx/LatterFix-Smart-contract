//! On-chain audit log of state-root hashes after critical mutative calls
//! (issue #91).
//!
//! Rather than storing full state snapshots on-chain (expensive), this
//! module computes a compact SHA-256 "state root" over a mutated struct's
//! XDR encoding — the same `to_xdr` + `sha256` primitive `vault.rs` and
//! `verify_and_claim_batch_task` already use for Merkle leaves — and
//! appends it to a bounded, indexed on-chain log. Off-chain indexers can
//! replay events, recompute the same root from the state they observed,
//! and confirm it against the logged root via `verify_audit_root` without
//! the contract retaining full historical state.

use soroban_sdk::{contracttype, xdr::ToXdr, Address, BytesN, Env, Symbol, Vec};

#[contracttype]
pub enum AuditLogKey {
    Count,
    Entry(u32),
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, kani), derive(Debug))]
pub struct AuditLogEntry {
    pub index: u32,
    pub operation: Symbol,
    pub subject_id: u32,
    pub actor: Address,
    pub root_hash: BytesN<32>,
    pub ledger: u32,
    pub timestamp: u64,
}

/// Cap on how many entries `get_recent_audit_log` walks in one call.
pub const MAX_RECENT_LOOKUP: u32 = 100;

/// Compute the state root for any XDR-serializable value: SHA-256 of its
/// XDR encoding.
pub fn compute_state_root<T: ToXdr + Clone>(env: &Env, state: &T) -> BytesN<32> {
    let bytes = state.clone().to_xdr(env);
    env.crypto().sha256(&bytes).into()
}

/// Record a state-root entry after a critical mutative call. Storage is
/// indexed (`Entry(index)`) rather than one growing `Vec`, so a caller pays
/// only for the entries it actually reads instead of the whole log.
pub fn record_state_root(
    env: &Env,
    operation: Symbol,
    subject_id: u32,
    actor: Address,
    root_hash: BytesN<32>,
) -> AuditLogEntry {
    let index: u32 = env
        .storage()
        .instance()
        .get(&AuditLogKey::Count)
        .unwrap_or(0);

    let entry = AuditLogEntry {
        index,
        operation: operation.clone(),
        subject_id,
        actor,
        root_hash: root_hash.clone(),
        ledger: env.ledger().sequence(),
        timestamp: env.ledger().timestamp(),
    };

    env.storage()
        .persistent()
        .set(&AuditLogKey::Entry(index), &entry);
    crate::storage::extend_persistent_ttl(
        env,
        &AuditLogKey::Entry(index),
        100_000,
        crate::storage::DEFAULT_PERSISTENT_TTL,
    );
    env.storage()
        .instance()
        .set(&AuditLogKey::Count, &(index + 1));

    crate::events::emit_audit_root_recorded(env, operation, subject_id, root_hash, index);

    entry
}

/// Total number of audit-log entries ever recorded.
pub fn get_audit_log_count(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&AuditLogKey::Count)
        .unwrap_or(0)
}

/// Fetch one audit-log entry by index, if it exists.
pub fn get_audit_log_entry(env: &Env, index: u32) -> Option<AuditLogEntry> {
    env.storage().persistent().get(&AuditLogKey::Entry(index))
}

/// The most recent `limit` entries (newest first), capped at
/// `MAX_RECENT_LOOKUP` so a caller can't force an unbounded read.
pub fn get_recent_audit_log(env: &Env, limit: u32) -> Vec<AuditLogEntry> {
    let count = get_audit_log_count(env);
    let limit = limit.min(MAX_RECENT_LOOKUP).min(count);
    let mut out = Vec::new(env);
    let mut i = 0u32;
    while i < limit {
        let idx = count - 1 - i;
        if let Some(entry) = get_audit_log_entry(env, idx) {
            out.push_back(entry);
        }
        i += 1;
    }
    out
}

/// Verify that `expected_root` matches the state root actually logged at
/// `index` — the standardized verification query an off-chain indexer calls
/// after recomputing a root from observed events.
pub fn verify_audit_root(env: &Env, index: u32, expected_root: BytesN<32>) -> bool {
    match get_audit_log_entry(env, index) {
        Some(entry) => entry.root_hash == expected_root,
        None => false,
    }
}
