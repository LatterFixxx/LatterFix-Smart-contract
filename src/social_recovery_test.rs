#![cfg(test)]
#![allow(deprecated)]

//! Unit tests for the multi-signature admin-role recovery protocol
//! (`src/social_recovery.rs`).

use crate::reputation::ReputationEventType;
use crate::social_recovery::RecoveryStatus;
use crate::{DataKey, TaskManagerContract, TaskManagerContractClient};

use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, Env, Symbol};

struct Ctx<'a> {
    env: Env,
    client: TaskManagerContractClient<'a>,
    contract_id: Address,
    admin: Address,
}

fn setup() -> Ctx<'static> {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, TaskManagerContract);
    let client = TaskManagerContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(token_admin);
    let fee_recipient = Address::generate(&env);
    client.initialize(&admin, &100u32, &token, &fee_recipient);

    Ctx {
        env,
        client,
        contract_id,
        admin,
    }
}

/// A fresh address seeded with enough reputation to sit in the given tier.
/// `points` is added on top of the default starting reputation of 100.
fn seeded_user(ctx: &Ctx, points: i32) -> Address {
    let user = Address::generate(&ctx.env);
    let env = ctx.env.clone();
    let u = user.clone();
    ctx.env.as_contract(&ctx.contract_id, move || {
        crate::reputation::award_reputation(
            env.clone(),
            u,
            points,
            ReputationEventType::BonusReward,
            None,
            Symbol::new(&env, "seed"),
        );
    });
    user
}

/// Master tier is 1000..=2499 points.
fn guardian(ctx: &Ctx) -> Address {
    seeded_user(ctx, 1_500)
}

fn set_admin_directly(ctx: &Ctx, new_admin: &Address) {
    let na = new_admin.clone();
    ctx.env.as_contract(&ctx.contract_id, || {
        ctx.env.storage().instance().set(&DataKey::Admin, &na);
    });
}

// ── Successful recovery ────────────────────────────────────────────────────

#[test]
fn test_successful_recovery_rotates_admin() {
    let ctx = setup();
    let g1 = guardian(&ctx);
    let g2 = guardian(&ctx);
    let g3 = guardian(&ctx);
    let new_admin = Address::generate(&ctx.env);

    let id = ctx.client.propose_recovery(&g1, &new_admin); // g1 auto-approves
    assert_eq!(ctx.client.approve_recovery(&g2, &id), RecoveryStatus::Pending);
    assert_eq!(ctx.client.approve_recovery(&g3, &id), RecoveryStatus::Approved);

    let returned = ctx.client.execute_recovery(&g1, &id);
    assert_eq!(returned, new_admin);
    assert_eq!(ctx.client.get_admin(), new_admin);

    // The rotation really took effect: old admin loses admin powers, new one gains them.
    assert!(ctx
        .client
        .try_configure_recovery(&ctx.admin, &3, &1000)
        .is_err());
    ctx.client.configure_recovery(&new_admin, &2, &1000);

    // Proposal is terminal and no longer active.
    assert_eq!(
        ctx.client.get_recovery_proposal(&id).unwrap().status,
        RecoveryStatus::Executed
    );
    assert!(ctx.client.get_active_recovery().is_none());
}

#[test]
fn test_recovery_with_custom_threshold_of_1() {
    let ctx = setup();
    ctx.client.configure_recovery(&ctx.admin, &1, &604_800);
    let g1 = guardian(&ctx);
    let new_admin = Address::generate(&ctx.env);

    let id = ctx.client.propose_recovery(&g1, &new_admin);
    assert_eq!(
        ctx.client.get_recovery_proposal(&id).unwrap().status,
        RecoveryStatus::Approved
    );
    ctx.client.execute_recovery(&g1, &id);
    assert_eq!(ctx.client.get_admin(), new_admin);
}

// ── Eligibility gating ─────────────────────────────────────────────────────

#[test]
#[should_panic]
fn test_proposer_below_tier_rejected() {
    let ctx = setup();
    let weak = seeded_user(&ctx, 200); // Contributor tier (100..=499)
    let new_admin = Address::generate(&ctx.env);
    ctx.client.propose_recovery(&weak, &new_admin);
}

#[test]
#[should_panic]
fn test_approver_below_tier_rejected() {
    let ctx = setup();
    let g1 = guardian(&ctx);
    let weak = seeded_user(&ctx, 300);
    let new_admin = Address::generate(&ctx.env);
    let id = ctx.client.propose_recovery(&g1, &new_admin);
    ctx.client.approve_recovery(&weak, &id);
}

#[test]
fn test_is_recovery_guardian_view() {
    let ctx = setup();
    let g = guardian(&ctx);
    let legend = seeded_user(&ctx, 5_000);
    let weak = seeded_user(&ctx, 100);
    assert!(ctx.client.is_recovery_guardian(&g));
    assert!(ctx.client.is_recovery_guardian(&legend));
    assert!(!ctx.client.is_recovery_guardian(&weak));
}

// ── State machine / transition validations ─────────────────────────────────

#[test]
#[should_panic]
fn test_threshold_not_met_cannot_execute() {
    let ctx = setup();
    let g1 = guardian(&ctx);
    let g2 = guardian(&ctx);
    let new_admin = Address::generate(&ctx.env);
    let id = ctx.client.propose_recovery(&g1, &new_admin);
    ctx.client.approve_recovery(&g2, &id); // 2/3 only
    ctx.client.execute_recovery(&g1, &id);
}

#[test]
fn test_threshold_not_met_status_still_pending() {
    let ctx = setup();
    let g1 = guardian(&ctx);
    let g2 = guardian(&ctx);
    let new_admin = Address::generate(&ctx.env);
    let id = ctx.client.propose_recovery(&g1, &new_admin);
    ctx.client.approve_recovery(&g2, &id);
    assert_eq!(
        ctx.client.get_recovery_proposal(&id).unwrap().status,
        RecoveryStatus::Pending
    );
}

#[test]
#[should_panic]
fn test_double_approve_rejected() {
    let ctx = setup();
    let g1 = guardian(&ctx);
    let g2 = guardian(&ctx);
    let new_admin = Address::generate(&ctx.env);
    let id = ctx.client.propose_recovery(&g1, &new_admin);
    ctx.client.approve_recovery(&g2, &id);
    ctx.client.approve_recovery(&g2, &id);
}

#[test]
#[should_panic]
fn test_proposer_cannot_re_approve() {
    let ctx = setup();
    let g1 = guardian(&ctx);
    let new_admin = Address::generate(&ctx.env);
    let id = ctx.client.propose_recovery(&g1, &new_admin);
    ctx.client.approve_recovery(&g1, &id); // already auto-approved at propose
}

#[test]
#[should_panic]
fn test_double_execute_rejected() {
    let ctx = setup();
    ctx.client.configure_recovery(&ctx.admin, &1, &604_800);
    let g1 = guardian(&ctx);
    let new_admin = Address::generate(&ctx.env);
    let id = ctx.client.propose_recovery(&g1, &new_admin);
    ctx.client.execute_recovery(&g1, &id);
    ctx.client.execute_recovery(&g1, &id);
}

#[test]
#[should_panic]
fn test_cannot_execute_pending() {
    let ctx = setup();
    let g1 = guardian(&ctx);
    let new_admin = Address::generate(&ctx.env);
    let id = ctx.client.propose_recovery(&g1, &new_admin);
    ctx.client.execute_recovery(&g1, &id); // still Pending (1/3)
}

#[test]
#[should_panic]
fn test_expired_proposal_cannot_be_approved() {
    let ctx = setup();
    let g1 = guardian(&ctx);
    let g2 = guardian(&ctx);
    let new_admin = Address::generate(&ctx.env);
    let id = ctx.client.propose_recovery(&g1, &new_admin);

    ctx.env.ledger().set_timestamp(ctx.env.ledger().timestamp() + 604_800 + 1);
    ctx.client.approve_recovery(&g2, &id);
}

#[test]
#[should_panic]
fn test_expired_approved_proposal_cannot_execute() {
    let ctx = setup();
    ctx.client.configure_recovery(&ctx.admin, &1, &1_000);
    let g1 = guardian(&ctx);
    let new_admin = Address::generate(&ctx.env);
    let id = ctx.client.propose_recovery(&g1, &new_admin);

    ctx.env.ledger().set_timestamp(ctx.env.ledger().timestamp() + 2_000);
    ctx.client.execute_recovery(&g1, &id);
}

// ── Role transition validations ───────────────────────────────────────────

#[test]
#[should_panic]
fn test_propose_same_admin_rejected() {
    let ctx = setup();
    let g1 = guardian(&ctx);
    let admin = ctx.client.get_admin();
    ctx.client.propose_recovery(&g1, &admin);
}

#[test]
#[should_panic]
fn test_stale_execution_rejected_when_admin_moved() {
    let ctx = setup();
    let g1 = guardian(&ctx);
    let g2 = guardian(&ctx);
    let g3 = guardian(&ctx);
    let new_admin = Address::generate(&ctx.env);
    let id = ctx.client.propose_recovery(&g1, &new_admin);
    ctx.client.approve_recovery(&g2, &id);
    ctx.client.approve_recovery(&g3, &id); // Approved

    // Admin rotated by some other path since the snapshot.
    let someone_else = Address::generate(&ctx.env);
    set_admin_directly(&ctx, &someone_else);

    ctx.client.execute_recovery(&g1, &id);
}

#[test]
fn test_execution_revalidates_guardian_set() {
    let ctx = setup();
    let g1 = guardian(&ctx);
    let g2 = guardian(&ctx);
    let g3 = guardian(&ctx);
    let new_admin = Address::generate(&ctx.env);
    let id = ctx.client.propose_recovery(&g1, &new_admin);
    ctx.client.approve_recovery(&g2, &id);
    ctx.client.approve_recovery(&g3, &id); // Approved with 3/3

    // g3 loses standing (heavy penalty pushes below Master).
    let env = ctx.env.clone();
    let g3c = g3.clone();
    ctx.env.as_contract(&ctx.contract_id, move || {
        crate::reputation::award_reputation(
            env.clone(),
            g3c,
            -1_400,
            ReputationEventType::Penalty,
            None,
            Symbol::new(&env, "slash"),
        );
    });

    // Only 2 live approvals now < threshold 3 → execution refused.
    assert!(ctx.client.try_execute_recovery(&g1, &id).is_err());
}

// ── Single-active-proposal invariant ──────────────────────────────────────

#[test]
#[should_panic]
fn test_only_one_active_recovery() {
    let ctx = setup();
    let g1 = guardian(&ctx);
    let g2 = guardian(&ctx);
    let a1 = Address::generate(&ctx.env);
    let a2 = Address::generate(&ctx.env);
    ctx.client.propose_recovery(&g1, &a1);
    ctx.client.propose_recovery(&g2, &a2);
}

#[test]
fn test_new_proposal_allowed_after_cancel() {
    let ctx = setup();
    let g1 = guardian(&ctx);
    let a1 = Address::generate(&ctx.env);
    let a2 = Address::generate(&ctx.env);

    let id1 = ctx.client.propose_recovery(&g1, &a1);
    ctx.client.cancel_recovery(&ctx.admin, &id1); // sitting admin vetoes
    assert_eq!(
        ctx.client.get_recovery_proposal(&id1).unwrap().status,
        RecoveryStatus::Cancelled
    );
    assert!(ctx.client.get_active_recovery().is_none());

    let id2 = ctx.client.propose_recovery(&g1, &a2);
    assert_eq!(id2, id1 + 1);
}

#[test]
fn test_stale_active_proposal_is_expired_by_next_propose() {
    let ctx = setup();
    let g1 = guardian(&ctx);
    let a1 = Address::generate(&ctx.env);
    let a2 = Address::generate(&ctx.env);

    let id1 = ctx.client.propose_recovery(&g1, &a1);
    ctx.env.ledger().set_timestamp(ctx.env.ledger().timestamp() + 604_800 + 1);

    let id2 = ctx.client.propose_recovery(&g1, &a2);
    assert_eq!(
        ctx.client.get_recovery_proposal(&id1).unwrap().status,
        RecoveryStatus::Expired
    );
    assert_eq!(
        ctx.client.get_active_recovery().unwrap().id,
        id2
    );
}

// ── Cancellation authorization ────────────────────────────────────────────

#[test]
#[should_panic]
fn test_stranger_cannot_cancel() {
    let ctx = setup();
    let g1 = guardian(&ctx);
    let stranger = guardian(&ctx);
    let a1 = Address::generate(&ctx.env);
    let id = ctx.client.propose_recovery(&g1, &a1);
    ctx.client.cancel_recovery(&stranger, &id);
}

#[test]
fn test_proposer_can_cancel_own_proposal() {
    let ctx = setup();
    let g1 = guardian(&ctx);
    let a1 = Address::generate(&ctx.env);
    let id = ctx.client.propose_recovery(&g1, &a1);
    ctx.client.cancel_recovery(&g1, &id);
    assert_eq!(
        ctx.client.get_recovery_proposal(&id).unwrap().status,
        RecoveryStatus::Cancelled
    );
}

// ── Configuration ────────────────────────────────────────────────────────

#[test]
#[should_panic]
fn test_configure_recovery_admin_only() {
    let ctx = setup();
    let not_admin = guardian(&ctx);
    ctx.client.configure_recovery(&not_admin, &2, &1000);
}

#[test]
#[should_panic]
fn test_configure_recovery_rejects_zero_threshold() {
    let ctx = setup();
    ctx.client.configure_recovery(&ctx.admin, &0, &1000);
}

#[test]
fn test_default_config_when_unset() {
    let ctx = setup();
    let cfg = ctx.client.get_recovery_config();
    assert_eq!(cfg.threshold, 3);
    assert_eq!(cfg.proposal_ttl, 604_800);
}
