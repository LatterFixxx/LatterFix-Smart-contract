#![cfg(test)]
#![allow(deprecated)]

use soroban_sdk::unwrap::UnwrapOptimized;
use crate::multisig::{MultisigAction, MultisigProposalStatus};
use crate::{TaskManagerContract, TaskManagerContractClient};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::token::StellarAssetClient;
use soroban_sdk::{Address, Env, Symbol, Vec};

// ── Shared setup ───────────────────────────────────────────────────────────

#[allow(dead_code)]
struct Ctx {
    client: TaskManagerContractClient<'static>,
    contract_id: Address,
    admin: Address,
    token: Address,
    token_admin: Address,
    fee_recipient: Address,
    signers: Vec<Address>,
}

fn setup(env: &Env, signer_count: u32, threshold: u32) -> Ctx {
    let contract_id = env.register_contract(None, TaskManagerContract);
    let client = TaskManagerContractClient::new(env, &contract_id);

    let admin = Address::generate(env);
    let token_admin = Address::generate(env);
    let token = env.register_stellar_asset_contract(token_admin.clone());
    let fee_recipient = Address::generate(env);

    client.initialize(&admin, &100u32, &token, &fee_recipient);

    let mut signers = Vec::new(env);
    for _ in 0..signer_count {
        signers.push_back(Address::generate(env));
    }

    client.configure_multisig(&admin, &signers, &threshold, &None, &None);

    Ctx {
        client,
        contract_id,
        admin,
        token,
        token_admin,
        fee_recipient,
        signers,
    }
}

fn signer(ctx: &Ctx, i: u32) -> Address {
    ctx.signers.get(i).unwrap_optimized()
}

fn desc(env: &Env) -> Symbol {
    Symbol::new(&env, "raise_platform_fee_to_2_5_")
}

// ── Configuration ──────────────────────────────────────────────────────────

#[test]
fn test_configure_multisig_stores_signer_set() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 3, 2);

    let config = ctx.client.get_multisig_config();
    assert_eq!(config.signers.len(), 3);
    assert_eq!(config.threshold, 2);
    assert!(config.auto_execute, "auto_execute should default to true");

    assert!(ctx.client.is_multisig_signer(&signer(&ctx, 0)));
    assert!(!ctx.client.is_multisig_signer(&Address::generate(&env)));
}

#[test]
fn test_configure_multisig_rejects_non_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 3, 2);

    let impostor = Address::generate(&env);
    let mut signers = Vec::new(&env);
    signers.push_back(impostor.clone());

    let res = ctx
        .client
        .try_configure_multisig(&impostor, &signers, &1u32, &None, &None);
    assert!(res.is_err(), "non-admin must not reconfigure the multisig");
}

#[test]
fn test_configure_multisig_rejects_invalid_threshold() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 3, 2);

    // Threshold above signer count is unsatisfiable.
    let res = ctx
        .client
        .try_configure_multisig(&ctx.admin, &ctx.signers, &4u32, &None, &None);
    assert!(res.is_err(), "threshold above signer count must be rejected");

    // Zero threshold would let anything execute unapproved.
    let res = ctx
        .client
        .try_configure_multisig(&ctx.admin, &ctx.signers, &0u32, &None, &None);
    assert!(res.is_err(), "zero threshold must be rejected");
}

#[test]
fn test_configure_multisig_rejects_duplicate_signers() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 3, 2);

    let dup = Address::generate(&env);
    let mut signers = Vec::new(&env);
    signers.push_back(dup.clone());
    signers.push_back(dup);

    let res = ctx
        .client
        .try_configure_multisig(&ctx.admin, &signers, &2u32, &None, &None);
    assert!(res.is_err(), "duplicate signer must be rejected");
}

// ── Proposal creation ──────────────────────────────────────────────────────

#[test]
fn test_propose_creates_pending_proposal() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 3, 2);

    let id = ctx.client.multisig_propose(
        &signer(&ctx, 0),
        &desc(&env),
        &MultisigAction::SetPlatformFee(250),
    );
    assert_eq!(id, 1);

    let proposal = ctx.client.get_multisig_proposal(&id).unwrap_optimized();
    assert_eq!(proposal.status, MultisigProposalStatus::Pending);
    assert_eq!(proposal.threshold, 2);
    assert_eq!(proposal.proposer, signer(&ctx, 0));
    assert_eq!(proposal.executed_at, None);

    // Proposing must not auto-approve: approval is a separate authorized act.
    assert_eq!(
        proposal.approvals.len(),
        0,
        "proposer should not be auto-approved"
    );
    assert_eq!(ctx.client.get_multisig_approval_count(&id), 0);
}

#[test]
fn test_propose_rejects_non_signer() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 3, 2);

    let outsider = Address::generate(&env);
    let res = ctx.client.try_multisig_propose(
        &outsider,
        &desc(&env),
        &MultisigAction::SetPlatformFee(250),
    );
    assert!(res.is_err(), "non-signer must not create proposals");
}

#[test]
fn test_propose_rejects_invalid_action_upfront() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 3, 2);

    // Over the 10% cap — rejected at creation so signers never spend
    // approvals on a proposal that could only trap at execution.
    let res = ctx.client.try_multisig_propose(
        &signer(&ctx, 0),
        &desc(&env),
        &MultisigAction::SetPlatformFee(1001),
    );
    assert!(res.is_err(), "out-of-range fee must be rejected at proposal time");

    let res = ctx.client.try_multisig_propose(
        &signer(&ctx, 0),
        &desc(&env),
        &MultisigAction::TreasuryTransfer(ctx.token.clone(), ctx.fee_recipient.clone(), 0),
    );
    assert!(res.is_err(), "non-positive transfer must be rejected");
}

#[test]
fn test_proposal_ids_increment() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 3, 2);

    let a = ctx.client.multisig_propose(
        &signer(&ctx, 0),
        &desc(&env),
        &MultisigAction::SetPlatformFee(100),
    );
    let b = ctx.client.multisig_propose(
        &signer(&ctx, 1),
        &desc(&env),
        &MultisigAction::SetPlatformFee(200),
    );
    assert_eq!((a, b), (1, 2));
}

// ── Voting ─────────────────────────────────────────────────────────────────

#[test]
fn test_vote_below_threshold_stays_pending() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 3, 2);

    let id = ctx.client.multisig_propose(
        &signer(&ctx, 0),
        &desc(&env),
        &MultisigAction::SetPlatformFee(250),
    );

    let status = ctx.client.vote_proposal(&signer(&ctx, 0), &id);
    assert_eq!(status, MultisigProposalStatus::Pending);
    assert_eq!(ctx.client.get_multisig_approval_count(&id), 1);
    assert!(ctx.client.has_approved_proposal(&id, &signer(&ctx, 0)));
    assert!(!ctx.client.has_approved_proposal(&id, &signer(&ctx, 1)));
}

#[test]
fn test_vote_rejects_double_approval() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 3, 2);

    let id = ctx.client.multisig_propose(
        &signer(&ctx, 0),
        &desc(&env),
        &MultisigAction::SetPlatformFee(250),
    );

    ctx.client.vote_proposal(&signer(&ctx, 0), &id);
    let res = ctx.client.try_vote_proposal(&signer(&ctx, 0), &id);
    assert!(res.is_err(), "a signer must not approve the same proposal twice");
    assert_eq!(ctx.client.get_multisig_approval_count(&id), 1);
}

#[test]
fn test_vote_rejects_non_signer() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 3, 2);

    let id = ctx.client.multisig_propose(
        &signer(&ctx, 0),
        &desc(&env),
        &MultisigAction::SetPlatformFee(250),
    );

    let outsider = Address::generate(&env);
    let res = ctx.client.try_vote_proposal(&outsider, &id);
    assert!(res.is_err(), "non-signer approval must be rejected");
}

#[test]
fn test_vote_rejects_unknown_proposal() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 3, 2);

    let res = ctx.client.try_vote_proposal(&signer(&ctx, 0), &999u32);
    assert!(res.is_err(), "voting on a missing proposal must fail");
}

// ── Threshold execution ────────────────────────────────────────────────────

#[test]
fn test_threshold_auto_executes_parameter_change() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 3, 2);

    let id = ctx.client.multisig_propose(
        &signer(&ctx, 0),
        &desc(&env),
        &MultisigAction::SetPlatformFee(250),
    );

    assert_eq!(
        ctx.client.vote_proposal(&signer(&ctx, 0), &id),
        MultisigProposalStatus::Pending
    );

    // Second approval hits the threshold and applies the change in-call.
    assert_eq!(
        ctx.client.vote_proposal(&signer(&ctx, 1), &id),
        MultisigProposalStatus::Executed
    );

    let proposal = ctx.client.get_multisig_proposal(&id).unwrap_optimized();
    assert_eq!(proposal.status, MultisigProposalStatus::Executed);
    assert!(proposal.executed_at.is_some());
    assert_eq!(proposal.approvals.len(), 2);

    // The parameter change actually landed in contract state.
    assert_eq!(fee_bps(&env, &ctx), 250);
}

#[test]
fn test_executed_proposal_is_terminal() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 3, 2);

    let id = ctx.client.multisig_propose(
        &signer(&ctx, 0),
        &desc(&env),
        &MultisigAction::SetPlatformFee(250),
    );
    ctx.client.vote_proposal(&signer(&ctx, 0), &id);
    ctx.client.vote_proposal(&signer(&ctx, 1), &id);

    // No further votes, no re-execution.
    let res = ctx.client.try_vote_proposal(&signer(&ctx, 2), &id);
    assert!(res.is_err(), "executed proposal must not accept more votes");

    let res = ctx
        .client
        .try_multisig_execute_proposal(&signer(&ctx, 0), &id);
    assert!(res.is_err(), "executed proposal must not re-execute");
}

#[test]
fn test_manual_execution_when_auto_execute_disabled() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 3, 2);

    // Reconfigure with auto_execute off, keeping the same signer set.
    ctx.client
        .configure_multisig(&ctx.admin, &ctx.signers, &2u32, &None, &Some(false));

    let id = ctx.client.multisig_propose(
        &signer(&ctx, 0),
        &desc(&env),
        &MultisigAction::SetPlatformFee(250),
    );
    ctx.client.vote_proposal(&signer(&ctx, 0), &id);

    // Threshold reached, but execution is deferred.
    let status = ctx.client.vote_proposal(&signer(&ctx, 1), &id);
    assert_eq!(status, MultisigProposalStatus::Approved);
    assert_eq!(fee_bps(&env, &ctx), 100, "fee must not change before execution");

    let status = ctx
        .client
        .multisig_execute_proposal(&signer(&ctx, 2), &id);
    assert_eq!(status, MultisigProposalStatus::Executed);
    assert_eq!(fee_bps(&env, &ctx), 250);
}

#[test]
fn test_execute_rejects_pending_proposal() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 3, 2);

    let id = ctx.client.multisig_propose(
        &signer(&ctx, 0),
        &desc(&env),
        &MultisigAction::SetPlatformFee(250),
    );
    ctx.client.vote_proposal(&signer(&ctx, 0), &id);

    let res = ctx
        .client
        .try_multisig_execute_proposal(&signer(&ctx, 0), &id);
    assert!(res.is_err(), "under-approved proposal must not execute");
}

#[test]
fn test_single_signer_threshold_executes_on_first_vote() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 1, 1);

    let id = ctx.client.multisig_propose(
        &signer(&ctx, 0),
        &desc(&env),
        &MultisigAction::SetPlatformFee(500),
    );

    // Creation alone must not execute; the approval is what triggers it.
    assert_eq!(fee_bps(&env, &ctx), 100);

    let status = ctx.client.vote_proposal(&signer(&ctx, 0), &id);
    assert_eq!(status, MultisigProposalStatus::Executed);
    assert_eq!(fee_bps(&env, &ctx), 500);
}

// ── Treasury movements ─────────────────────────────────────────────────────

#[test]
fn test_treasury_transfer_executes_on_threshold() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 3, 2);

    // Fund the contract treasury.
    let minter = StellarAssetClient::new(&env, &ctx.token);
    minter.mint(&ctx.contract_id, &1_000i128);

    let recipient = Address::generate(&env);
    let token_client = soroban_sdk::token::Client::new(&env, &ctx.token);
    assert_eq!(token_client.balance(&recipient), 0);

    let id = ctx.client.multisig_propose(
        &signer(&ctx, 0),
        &Symbol::new(&env, "pay_grant"),
        &MultisigAction::TreasuryTransfer(ctx.token.clone(), recipient.clone(), 400),
    );

    ctx.client.vote_proposal(&signer(&ctx, 0), &id);
    let status = ctx.client.vote_proposal(&signer(&ctx, 1), &id);

    assert_eq!(status, MultisigProposalStatus::Executed);
    assert_eq!(token_client.balance(&recipient), 400);
    assert_eq!(token_client.balance(&ctx.contract_id), 600);
}

#[test]
fn test_treasury_transfer_beyond_balance_reverts_whole_call() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 3, 2);

    let minter = StellarAssetClient::new(&env, &ctx.token);
    minter.mint(&ctx.contract_id, &100i128);

    let recipient = Address::generate(&env);
    let id = ctx.client.multisig_propose(
        &signer(&ctx, 0),
        &Symbol::new(&env, "overdraw"),
        &MultisigAction::TreasuryTransfer(ctx.token.clone(), recipient.clone(), 5_000),
    );

    ctx.client.vote_proposal(&signer(&ctx, 0), &id);

    // The threshold-reaching vote triggers execution, which traps on the
    // underfunded transfer and reverts the approval along with it.
    let res = ctx.client.try_vote_proposal(&signer(&ctx, 1), &id);
    assert!(res.is_err(), "underfunded treasury transfer must revert");

    let proposal = ctx.client.get_multisig_proposal(&id).unwrap_optimized();
    assert_eq!(
        proposal.status,
        MultisigProposalStatus::Pending,
        "failed execution must leave the proposal pending"
    );
    assert_eq!(
        proposal.approvals.len(),
        1,
        "the reverted approval must not be recorded"
    );

    let token_client = soroban_sdk::token::Client::new(&env, &ctx.token);
    assert_eq!(token_client.balance(&ctx.contract_id), 100);
    assert_eq!(token_client.balance(&recipient), 0);
}

// ── Signer rotation ────────────────────────────────────────────────────────

#[test]
fn test_signer_rotation_via_proposal() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 3, 2);

    let new_signer = Address::generate(&env);
    let mut new_set = Vec::new(&env);
    new_set.push_back(signer(&ctx, 0));
    new_set.push_back(new_signer.clone());

    let id = ctx.client.multisig_propose(
        &signer(&ctx, 0),
        &Symbol::new(&env, "rotate_signers"),
        &MultisigAction::SetSigners(new_set, 2),
    );

    ctx.client.vote_proposal(&signer(&ctx, 0), &id);
    let status = ctx.client.vote_proposal(&signer(&ctx, 1), &id);
    assert_eq!(status, MultisigProposalStatus::Executed);

    let config = ctx.client.get_multisig_config();
    assert_eq!(config.signers.len(), 2);
    assert!(ctx.client.is_multisig_signer(&new_signer));
    assert!(
        !ctx.client.is_multisig_signer(&signer(&ctx, 2)),
        "rotated-out signer must lose authority"
    );
}

#[test]
fn test_approval_from_removed_signer_stops_counting() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 4, 3);

    // Signer 3 approves a fee change while still in the set.
    let fee_id = ctx.client.multisig_propose(
        &signer(&ctx, 0),
        &desc(&env),
        &MultisigAction::SetPlatformFee(250),
    );
    ctx.client.vote_proposal(&signer(&ctx, 3), &fee_id);
    assert_eq!(ctx.client.get_multisig_approval_count(&fee_id), 1);

    // Admin rotates signer 3 out.
    let mut new_set = Vec::new(&env);
    new_set.push_back(signer(&ctx, 0));
    new_set.push_back(signer(&ctx, 1));
    new_set.push_back(signer(&ctx, 2));
    ctx.client
        .configure_multisig(&ctx.admin, &new_set, &3u32, &None, &None);

    // Their stale approval no longer counts toward the threshold.
    assert_eq!(
        ctx.client.get_multisig_approval_count(&fee_id),
        0,
        "removed signer's approval must be discounted"
    );

    // So two remaining approvals are not enough for a threshold of 3.
    ctx.client.vote_proposal(&signer(&ctx, 0), &fee_id);
    let status = ctx.client.vote_proposal(&signer(&ctx, 1), &fee_id);
    assert_eq!(
        status,
        MultisigProposalStatus::Pending,
        "stale approval must not carry the proposal over the threshold"
    );

    // The third live approval does.
    let status = ctx.client.vote_proposal(&signer(&ctx, 2), &fee_id);
    assert_eq!(status, MultisigProposalStatus::Executed);
}

#[test]
fn test_threshold_is_snapshotted_at_creation() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 4, 4);

    let id = ctx.client.multisig_propose(
        &signer(&ctx, 0),
        &desc(&env),
        &MultisigAction::SetPlatformFee(250),
    );
    assert_eq!(ctx.client.get_multisig_proposal(&id).unwrap_optimized().threshold, 4);

    // Admin lowers the live threshold to 2 after the proposal was created.
    ctx.client
        .configure_multisig(&ctx.admin, &ctx.signers, &2u32, &None, &None);

    ctx.client.vote_proposal(&signer(&ctx, 0), &id);
    let status = ctx.client.vote_proposal(&signer(&ctx, 1), &id);
    assert_eq!(
        status,
        MultisigProposalStatus::Pending,
        "lowering the threshold must not retroactively approve a live proposal"
    );

    ctx.client.vote_proposal(&signer(&ctx, 2), &id);
    let status = ctx.client.vote_proposal(&signer(&ctx, 3), &id);
    assert_eq!(
        status,
        MultisigProposalStatus::Executed,
        "the snapshotted threshold of 4 governs"
    );
}

// ── Cancellation ───────────────────────────────────────────────────────────

#[test]
fn test_proposer_can_cancel() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 3, 2);

    let id = ctx.client.multisig_propose(
        &signer(&ctx, 0),
        &desc(&env),
        &MultisigAction::SetPlatformFee(250),
    );
    ctx.client.multisig_cancel_proposal(&signer(&ctx, 0), &id);

    let proposal = ctx.client.get_multisig_proposal(&id).unwrap_optimized();
    assert_eq!(proposal.status, MultisigProposalStatus::Cancelled);

    let res = ctx.client.try_vote_proposal(&signer(&ctx, 1), &id);
    assert!(res.is_err(), "cancelled proposal must not accept votes");
}

#[test]
fn test_admin_can_cancel_any_proposal() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 3, 2);

    let id = ctx.client.multisig_propose(
        &signer(&ctx, 0),
        &desc(&env),
        &MultisigAction::SetPlatformFee(250),
    );
    ctx.client.multisig_cancel_proposal(&ctx.admin, &id);

    assert_eq!(
        ctx.client.get_multisig_proposal(&id).unwrap_optimized().status,
        MultisigProposalStatus::Cancelled
    );
}

#[test]
fn test_unrelated_signer_cannot_cancel() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 3, 2);

    let id = ctx.client.multisig_propose(
        &signer(&ctx, 0),
        &desc(&env),
        &MultisigAction::SetPlatformFee(250),
    );

    let res = ctx
        .client
        .try_multisig_cancel_proposal(&signer(&ctx, 1), &id);
    assert!(res.is_err(), "only proposer or admin may cancel");
}

#[test]
fn test_cancel_after_approval_blocks_execution() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 3, 2);

    ctx.client
        .configure_multisig(&ctx.admin, &ctx.signers, &2u32, &None, &Some(false));

    let id = ctx.client.multisig_propose(
        &signer(&ctx, 0),
        &desc(&env),
        &MultisigAction::SetPlatformFee(250),
    );
    ctx.client.vote_proposal(&signer(&ctx, 0), &id);
    ctx.client.vote_proposal(&signer(&ctx, 1), &id);

    ctx.client.multisig_cancel_proposal(&ctx.admin, &id);

    let res = ctx
        .client
        .try_multisig_execute_proposal(&signer(&ctx, 0), &id);
    assert!(res.is_err(), "cancelled proposal must not execute");
    assert_eq!(fee_bps(&env, &ctx), 100);
}

#[test]
fn test_cancel_rejects_executed_proposal() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 3, 2);

    let id = ctx.client.multisig_propose(
        &signer(&ctx, 0),
        &desc(&env),
        &MultisigAction::SetPlatformFee(250),
    );
    ctx.client.vote_proposal(&signer(&ctx, 0), &id);
    ctx.client.vote_proposal(&signer(&ctx, 1), &id);

    let res = ctx.client.try_multisig_cancel_proposal(&ctx.admin, &id);
    assert!(res.is_err(), "executed proposal must not be cancellable");
}

// ── Expiry ─────────────────────────────────────────────────────────────────

#[test]
fn test_expired_proposal_rejects_votes() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 3, 2);

    let id = ctx.client.multisig_propose(
        &signer(&ctx, 0),
        &desc(&env),
        &MultisigAction::SetPlatformFee(250),
    );
    ctx.client.vote_proposal(&signer(&ctx, 0), &id);

    // Past the default 7-day TTL.
    env.ledger().with_mut(|l| l.timestamp += 604_800 + 1);

    let res = ctx.client.try_vote_proposal(&signer(&ctx, 1), &id);
    assert!(res.is_err(), "expired proposal must not accept votes");
}

#[test]
fn test_expired_proposal_rejects_execution() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 3, 2);

    ctx.client
        .configure_multisig(&ctx.admin, &ctx.signers, &2u32, &Some(1_000u64), &Some(false));

    let id = ctx.client.multisig_propose(
        &signer(&ctx, 0),
        &desc(&env),
        &MultisigAction::SetPlatformFee(250),
    );
    ctx.client.vote_proposal(&signer(&ctx, 0), &id);
    let status = ctx.client.vote_proposal(&signer(&ctx, 1), &id);
    assert_eq!(status, MultisigProposalStatus::Approved);

    env.ledger().with_mut(|l| l.timestamp += 1_001);

    let res = ctx
        .client
        .try_multisig_execute_proposal(&signer(&ctx, 0), &id);
    assert!(res.is_err(), "expired proposal must not execute");
    assert_eq!(fee_bps(&env, &ctx), 100);
}

// ── Listing ────────────────────────────────────────────────────────────────

#[test]
fn test_pending_proposals_listing() {
    let env = Env::default();
    env.mock_all_auths();
    let ctx = setup(&env, 3, 2);

    let keep = ctx.client.multisig_propose(
        &signer(&ctx, 0),
        &desc(&env),
        &MultisigAction::SetPlatformFee(250),
    );
    let cancel = ctx.client.multisig_propose(
        &signer(&ctx, 0),
        &desc(&env),
        &MultisigAction::SetPlatformFee(300),
    );
    let execute = ctx.client.multisig_propose(
        &signer(&ctx, 0),
        &desc(&env),
        &MultisigAction::SetPlatformFee(400),
    );

    ctx.client.multisig_cancel_proposal(&signer(&ctx, 0), &cancel);
    ctx.client.vote_proposal(&signer(&ctx, 0), &execute);
    ctx.client.vote_proposal(&signer(&ctx, 1), &execute);

    let pending = ctx.client.get_pending_multisig_proposals();
    assert_eq!(pending.len(), 1, "only the untouched proposal stays pending");
    assert_eq!(pending.get(0).unwrap_optimized().id, keep);
}

// ── Helpers ────────────────────────────────────────────────────────────────

fn fee_bps(env: &Env, ctx: &Ctx) -> u32 {
    env.as_contract(&ctx.contract_id, || {
        env.storage()
            .instance()
            .get(&crate::DataKey::PlatformFeeBps)
            .unwrap_optimized()
    })
}
