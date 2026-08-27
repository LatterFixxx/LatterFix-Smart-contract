use soroban_sdk::unwrap::UnwrapOptimized;
use soroban_sdk::{contracttype, Address, Env, Symbol, Vec};

use crate::{reputation, DataKey};

// Emergency social-recovery protocol for the contract `admin` role.
//
// The contract has a single `admin` address (`DataKey::Admin`) that gates fee
// changes, dispute resolution, pausing, upgrades and more. If that key is lost
// or compromised there is otherwise no way back. This module lets a **threshold
// of top-tier community members** (reputation tier `Master` or `Legend`)
// collectively rotate the admin to a new address.
//
// It is deliberately separate from `multisig`, which governs routine admin
// *actions* through an admin-appointed signer set. Here the "signers" are not
// appointed — eligibility is earned reputation — and the only action is the
// admin rotation itself, used when the normal admin path is unavailable.
//
// Guardians approve **on-chain**: each `approve_recovery` call is authorized by
// the guardian's own key, which the host verifies cryptographically — the same
// threshold-signature model `multisig` uses.
//
// ### State machine
//
// ```text
//   Pending ──(approvals >= threshold)──> Approved ──(execute)──> Executed
//      │                                      │
//      ├───────────────(cancel)───────────────┤
//      │                                      │
//      └────(ttl elapsed → Expired on touch)──┘
// ```
//
// At most one non-terminal (`Pending` / `Approved`) proposal exists at a time.
// A stale one (past its TTL) is lazily marked `Expired` when a new proposal is
// filed, so recovery can always make progress.
//
// ### Safety properties
//
// * `threshold` is snapshotted at proposal creation — reconfiguring it can't
//   retroactively lower the bar for a live proposal.
// * Approvals are **re-counted at execution** against each approver's *current*
//   tier, so a guardian who dropped below `Master` stops counting.
// * The live admin is snapshotted at creation and re-checked at execution: if
//   the admin was already rotated by any other path, the stale proposal can't
//   execute (no double-rotation, no replay).
// * The sitting admin can `cancel_recovery` a malicious proposal at any time
//   before it executes.

// ============================================================================
// Types
// ============================================================================

#[contracttype]
#[derive(Clone, Copy, Eq, PartialEq)]
#[repr(u32)]
#[cfg_attr(any(test, kani), derive(Debug))]
pub enum RecoveryStatus {
    Pending = 0,
    Approved = 1,
    Executed = 2,
    Cancelled = 3,
    Expired = 4,
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, kani), derive(Debug))]
pub struct RecoveryProposal {
    pub id: u32,
    pub proposer: Address,
    pub proposed_admin: Address,
    /// Live admin captured when the proposal was filed.
    pub admin_at_proposal: Address,
    pub status: RecoveryStatus,
    /// Top-tier guardians who have approved (proposer included).
    pub approvals: Vec<Address>,
    /// Approval count required, snapshotted at creation.
    pub threshold: u32,
    pub created_at: u64,
    pub expires_at: u64,
    pub executed_at: Option<u64>,
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
pub struct RecoveryConfig {
    pub threshold: u32,
    pub proposal_ttl: u64,
}

#[contracttype]
pub enum RecoveryKey {
    Config,
    Proposal(u32),
    ProposalCount,
    ActiveProposal,
}

pub const DEFAULT_THRESHOLD: u32 = 3;
pub const DEFAULT_PROPOSAL_TTL: u64 = 604_800; // 7 days

// ============================================================================
// Authorization / eligibility
// ============================================================================

fn require_admin(env: &Env, caller: &Address) {
    caller.require_auth();
    let admin: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .unwrap_optimized();
    if *caller != admin {
        panic!();
    }
}

fn current_admin(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .unwrap_optimized()
}

/// True if `user` currently sits in the `Master` or `Legend` reputation tier.
pub fn is_eligible_guardian(env: &Env, user: &Address) -> bool {
    let tier = reputation::get_user_tier(env.clone(), user.clone());
    tier == Symbol::new(env, "Master") || tier == Symbol::new(env, "Legend")
}

fn require_guardian(env: &Env, user: &Address) {
    if !is_eligible_guardian(env, user) {
        panic!();
    }
}

// ============================================================================
// Configuration
// ============================================================================

pub fn configure_recovery(env: Env, admin: Address, threshold: u32, proposal_ttl: u64) {
    require_admin(&env, &admin);
    if threshold < 1 {
        panic!();
    }
    if proposal_ttl == 0 {
        panic!();
    }
    let config = RecoveryConfig {
        threshold,
        proposal_ttl,
    };
    env.storage().instance().set(&RecoveryKey::Config, &config);
}

pub fn get_config(env: &Env) -> RecoveryConfig {
    env.storage()
        .instance()
        .get(&RecoveryKey::Config)
        .unwrap_or(RecoveryConfig {
            threshold: DEFAULT_THRESHOLD,
            proposal_ttl: DEFAULT_PROPOSAL_TTL,
        })
}

// ============================================================================
// Proposal storage helpers
// ============================================================================

fn save_proposal(env: &Env, proposal: &RecoveryProposal) {
    let key = RecoveryKey::Proposal(proposal.id);
    env.storage().persistent().set(&key, proposal);
    crate::storage::extend_persistent_ttl(
        env,
        &key,
        100_000,
        crate::storage::DEFAULT_PERSISTENT_TTL,
    );
}

pub fn get_recovery_proposal(env: &Env, id: u32) -> Option<RecoveryProposal> {
    env.storage().persistent().get(&RecoveryKey::Proposal(id))
}

fn active_proposal_id(env: &Env) -> Option<u32> {
    env.storage().instance().get(&RecoveryKey::ActiveProposal)
}

fn set_active_proposal(env: &Env, id: Option<u32>) {
    match id {
        Some(v) => env.storage().instance().set(&RecoveryKey::ActiveProposal, &v),
        None => env.storage().instance().remove(&RecoveryKey::ActiveProposal),
    }
}

/// Count approvals from guardians who are *still* eligible right now.
fn live_approval_count(env: &Env, proposal: &RecoveryProposal) -> u32 {
    let mut valid = 0u32;
    for guardian in proposal.approvals.iter() {
        if is_eligible_guardian(env, &guardian) {
            valid += 1;
        }
    }
    valid
}

// ============================================================================
// Lifecycle
// ============================================================================

pub fn propose_recovery(env: Env, proposer: Address, proposed_admin: Address) -> u32 {
    proposer.require_auth();
    require_guardian(&env, &proposer);

    let admin = current_admin(&env);
    if proposed_admin == admin {
        panic!();
    }

    // At most one live proposal. If the existing one is stale, retire it.
    if let Some(active_id) = active_proposal_id(&env) {
        let mut active = get_recovery_proposal(&env, active_id).unwrap_optimized();
        let now = env.ledger().timestamp();
        let live = matches!(
            active.status,
            RecoveryStatus::Pending | RecoveryStatus::Approved
        );
        if live && now <= active.expires_at {
            panic!();
        }
        if live {
            active.status = RecoveryStatus::Expired;
            save_proposal(&env, &active);
        }
        set_active_proposal(&env, None);
    }

    let config = get_config(&env);
    let now = env.ledger().timestamp();

    let mut count: u32 = env
        .storage()
        .persistent()
        .get(&RecoveryKey::ProposalCount)
        .unwrap_or(0);
    count += 1;

    let mut approvals = Vec::new(&env);
    approvals.push_back(proposer.clone());

    let mut proposal = RecoveryProposal {
        id: count,
        proposer,
        proposed_admin,
        admin_at_proposal: admin,
        status: RecoveryStatus::Pending,
        approvals,
        threshold: config.threshold,
        created_at: now,
        expires_at: now + config.proposal_ttl,
        executed_at: None,
    };

    if proposal.threshold <= 1 {
        proposal.status = RecoveryStatus::Approved;
    }

    save_proposal(&env, &proposal);
    env.storage()
        .persistent()
        .set(&RecoveryKey::ProposalCount, &count);
    set_active_proposal(&env, Some(count));

    count
}

pub fn approve_recovery(env: Env, guardian: Address, recovery_id: u32) -> RecoveryStatus {
    guardian.require_auth();
    require_guardian(&env, &guardian);

    let mut proposal = get_recovery_proposal(&env, recovery_id).unwrap_optimized();

    if proposal.status != RecoveryStatus::Pending {
        panic!();
    }

    // Past its TTL the proposal is dead; `propose_recovery` will retire it.
    let now = env.ledger().timestamp();
    if now > proposal.expires_at {
        panic!();
    }

    if proposal.approvals.contains(&guardian) {
        panic!();
    }
    proposal.approvals.push_back(guardian);

    if live_approval_count(&env, &proposal) >= proposal.threshold {
        proposal.status = RecoveryStatus::Approved;
    }

    save_proposal(&env, &proposal);
    proposal.status
}

pub fn execute_recovery(env: Env, caller: Address, recovery_id: u32) -> Address {
    caller.require_auth();
    require_guardian(&env, &caller);

    let mut proposal = get_recovery_proposal(&env, recovery_id).unwrap_optimized();

    if proposal.status != RecoveryStatus::Approved {
        panic!();
    }

    let now = env.ledger().timestamp();
    if now > proposal.expires_at {
        panic!();
    }

    // Re-validate the guardian set against current tiers.
    if live_approval_count(&env, &proposal) < proposal.threshold {
        panic!();
    }

    // The admin must not have moved since the snapshot — otherwise this is a
    // stale rotation that would silently clobber a newer admin.
    let admin = current_admin(&env);
    if admin != proposal.admin_at_proposal {
        panic!();
    }

    env.storage()
        .instance()
        .set(&DataKey::Admin, &proposal.proposed_admin);

    proposal.status = RecoveryStatus::Executed;
    proposal.executed_at = Some(now);
    save_proposal(&env, &proposal);
    set_active_proposal(&env, None);

    proposal.proposed_admin
}

pub fn cancel_recovery(env: Env, caller: Address, recovery_id: u32) {
    caller.require_auth();

    let mut proposal = get_recovery_proposal(&env, recovery_id).unwrap_optimized();

    match proposal.status {
        RecoveryStatus::Pending | RecoveryStatus::Approved => {}
        _ => panic!(),
    }

    // Only the proposer or the sitting admin may cancel.
    let admin = current_admin(&env);
    if caller != proposal.proposer && caller != admin {
        panic!();
    }

    proposal.status = RecoveryStatus::Cancelled;
    save_proposal(&env, &proposal);
    set_active_proposal(&env, None);
}

// ============================================================================
// Views
// ============================================================================

pub fn get_active_recovery(env: &Env) -> Option<RecoveryProposal> {
    active_proposal_id(env).and_then(|id| get_recovery_proposal(env, id))
}
