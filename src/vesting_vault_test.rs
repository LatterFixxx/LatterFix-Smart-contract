#![cfg(test)]
#![allow(deprecated)]

use crate::{TaskManagerContract, TaskManagerContractClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::testutils::Ledger as _;
use soroban_sdk::token::StellarAssetClient;
use soroban_sdk::{Address, Env, String, Symbol, Vec};

// ── Shared setup helper ────────────────────────────────────────────────────

fn setup_initialized_contract(
    env: &Env,
    fee_bps: u32,
) -> (
    TaskManagerContractClient<'_>,
    Address, // contract_id
    Address, // admin
    Address, // token_contract
    Address, // fee_recipient
) {
    let contract_id = env.register_contract(None, TaskManagerContract);
    let client = TaskManagerContractClient::new(env, &contract_id);

    let admin = Address::generate(env);
    let token_admin = Address::generate(env);
    let token_contract = env.register_stellar_asset_contract(token_admin);
    let fee_recipient = Address::generate(env);

    client.initialize(&admin, &fee_bps, &token_contract, &fee_recipient);
    (client, contract_id, admin, token_contract, fee_recipient)
}

// Helper to create a task with a milestone approved with vesting
fn setup_vesting_vault(
    env: &Env,
    contract_id: &Address,
    client: &TaskManagerContractClient<'_>,
    token_contract: &Address,
    creator: &Address,
    assignee: &Address,
    milestone_amount: i128,
    vesting_period: u64,
) -> (u32, u32, u32) {
    let token_admin = StellarAssetClient::new(env, token_contract);
    token_admin.mint(creator, &(milestone_amount * 2));

    let mut tags = Vec::new(env);
    tags.push_back(Symbol::new(env, "test"));

    let task_id = client.create_task(
        creator,
        &Symbol::new(env, "Test_Task"),
        &Symbol::new(env, "Task_with_milestone"),
        &milestone_amount,
        &tags,
    );

    client.assign_task(assignee, &task_id);

    // Create milestone via escrow module within contract context
    let e = env.clone();
    let ms_title = Symbol::new(env, "Milestone_1");
    env.as_contract(contract_id, move || {
        crate::escrow::create_milestone(e, task_id, ms_title, milestone_amount, None);
    });

    // Submit milestone
    client.submit_milestone(
        assignee,
        &task_id,
        &1,
        &Symbol::new(env, "https___github_com_pr_1"),
    );

    // Approve milestone with vesting
    let vault_id = client.approve_milestone_with_vesting(
        creator,
        &task_id,
        &1,
        &vesting_period,
        &None,
    );

    (task_id, 1, vault_id)
}

// ── Test 1: Create vesting vault for approved milestone ────────────────────

#[test]
fn test_create_vesting_vault() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, contract_id, _, token_contract, _) =
        setup_initialized_contract(&env, 100);

    let creator = Address::generate(&env);
    let assignee = Address::generate(&env);

    let vesting_period = 86400u64; // 24 hours
    let (task_id, milestone_id, vault_id) = setup_vesting_vault(
        &env,
        &contract_id,
        &client,
        &token_contract,
        &creator,
        &assignee,
        500,
        vesting_period,
    );

    assert_eq!(vault_id, 1);

    // Verify vault details
    let vault = client.get_vesting_vault(&vault_id).unwrap();
    assert_eq!(vault.task_id, task_id);
    assert_eq!(vault.milestone_id, milestone_id);
    assert_eq!(vault.beneficiary, assignee);
    assert_eq!(vault.amount, 500);
    assert_eq!(vault.token, token_contract);
    assert_eq!(vault.status, crate::vesting_vault::VestingVaultStatus::Active);

    // Verify funds are still locked in contract
    let token = soroban_sdk::token::Client::new(&env, &token_contract);
    assert_eq!(token.balance(&contract_id), 500);
    assert_eq!(token.balance(&assignee), 0);
}

// ── Test 2: Cannot release before vesting ends ─────────────────────────────

#[test]
fn test_cannot_release_before_vesting_ends() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, contract_id, _, token_contract, _) =
        setup_initialized_contract(&env, 100);

    let creator = Address::generate(&env);
    let assignee = Address::generate(&env);

    let vesting_period = 86400u64; // 24 hours
    let (_, _, vault_id) = setup_vesting_vault(
        &env,
        &contract_id,
        &client,
        &token_contract,
        &creator,
        &assignee,
        500,
        vesting_period,
    );

    // Try to release before vesting ends - should fail
    let result = client.try_release_vesting_vault(&assignee, &vault_id);
    assert!(result.is_err(), "cannot release before vesting period ends");
}

// ── Test 3: Release after vesting ends ─────────────────────────────────────

#[test]
fn test_release_after_vesting_ends() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, contract_id, _, token_contract, _) =
        setup_initialized_contract(&env, 100);

    let creator = Address::generate(&env);
    let assignee = Address::generate(&env);

    let vesting_period = 86400u64; // 24 hours
    let (_, _, vault_id) = setup_vesting_vault(
        &env,
        &contract_id,
        &client,
        &token_contract,
        &creator,
        &assignee,
        500,
        vesting_period,
    );

    // Advance time past vesting period
    env.ledger().set_timestamp(env.ledger().timestamp() + vesting_period + 1);

    // Release funds
    let amount = client.release_vesting_vault(&assignee, &vault_id);
    assert_eq!(amount, 500);

    // Verify funds transferred
    let token = soroban_sdk::token::Client::new(&env, &token_contract);
    assert_eq!(token.balance(&assignee), 500);
    assert_eq!(token.balance(&contract_id), 0);

    // Verify vault status
    let vault = client.get_vesting_vault(&vault_id).unwrap();
    assert_eq!(vault.status, crate::vesting_vault::VestingVaultStatus::Released);
}

// ── Test 4: Dispute during vesting period ──────────────────────────────────

#[test]
fn test_dispute_during_vesting_period() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, contract_id, _, token_contract, _) =
        setup_initialized_contract(&env, 100);

    let creator = Address::generate(&env);
    let assignee = Address::generate(&env);

    let vesting_period = 86400u64; // 24 hours
    let (_, _, vault_id) = setup_vesting_vault(
        &env,
        &contract_id,
        &client,
        &token_contract,
        &creator,
        &assignee,
        500,
        vesting_period,
    );

    // Dispute during vesting period
    let reason = String::from_str(&env, "Quality issues found");
    client.dispute_vesting_vault(&creator, &vault_id, &reason);

    // Verify vault is disputed
    let vault = client.get_vesting_vault(&vault_id).unwrap();
    assert_eq!(vault.status, crate::vesting_vault::VestingVaultStatus::Disputed);
    assert_eq!(vault.disputed_by, Some(creator.clone()));
}

// ── Test 5: Cannot release disputed vault ──────────────────────────────────

#[test]
fn test_cannot_release_disputed_vault() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, contract_id, _, token_contract, _) =
        setup_initialized_contract(&env, 100);

    let creator = Address::generate(&env);
    let assignee = Address::generate(&env);

    let vesting_period = 86400u64; // 24 hours
    let (_, _, vault_id) = setup_vesting_vault(
        &env,
        &contract_id,
        &client,
        &token_contract,
        &creator,
        &assignee,
        500,
        vesting_period,
    );

    // Dispute
    let reason = String::from_str(&env, "Quality issues found");
    client.dispute_vesting_vault(&creator, &vault_id, &reason);

    // Advance time past vesting period
    env.ledger().set_timestamp(env.ledger().timestamp() + vesting_period + 1);

    // Try to release disputed vault - should fail
    let result = client.try_release_vesting_vault(&assignee, &vault_id);
    assert!(result.is_err(), "cannot release disputed vault");
}

// ── Test 6: Refund disputed vault ──────────────────────────────────────────

#[test]
fn test_refund_disputed_vault() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, contract_id, admin, token_contract, _) =
        setup_initialized_contract(&env, 100);

    let creator = Address::generate(&env);
    let assignee = Address::generate(&env);

    let vesting_period = 86400u64; // 24 hours
    let (_, _, vault_id) = setup_vesting_vault(
        &env,
        &contract_id,
        &client,
        &token_contract,
        &creator,
        &assignee,
        500,
        vesting_period,
    );

    // Dispute
    let reason = String::from_str(&env, "Quality issues found");
    client.dispute_vesting_vault(&creator, &vault_id, &reason);

    // Refund disputed vault
    let amount = client.refund_vesting_vault(&admin, &vault_id);
    assert_eq!(amount, 500);

    // Verify funds refunded to creator
    // Creator had 1000, used 500 for task, gets 500 back = 1000
    let token = soroban_sdk::token::Client::new(&env, &token_contract);
    assert_eq!(token.balance(&creator), 1000);
    assert_eq!(token.balance(&contract_id), 0);

    // Verify vault status
    let vault = client.get_vesting_vault(&vault_id).unwrap();
    assert_eq!(vault.status, crate::vesting_vault::VestingVaultStatus::Refunded);
}

// ── Test 7: Cannot dispute after vesting ends ──────────────────────────────

#[test]
fn test_cannot_dispute_after_vesting_ends() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, contract_id, _, token_contract, _) =
        setup_initialized_contract(&env, 100);

    let creator = Address::generate(&env);
    let assignee = Address::generate(&env);

    let vesting_period = 86400u64; // 24 hours
    let (_, _, vault_id) = setup_vesting_vault(
        &env,
        &contract_id,
        &client,
        &token_contract,
        &creator,
        &assignee,
        500,
        vesting_period,
    );

    // Advance time past vesting period
    env.ledger().set_timestamp(env.ledger().timestamp() + vesting_period + 1);

    // Try to dispute after vesting ends - should fail
    let reason = String::from_str(&env, "Too late to dispute");
    let result = client.try_dispute_vesting_vault(&creator, &vault_id, &reason);
    assert!(result.is_err(), "cannot dispute after vesting period ends");
}

// ── Test 8: Vesting time tracking ──────────────────────────────────────────

#[test]
fn test_vesting_time_tracking() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, contract_id, _, token_contract, _) =
        setup_initialized_contract(&env, 100);

    let creator = Address::generate(&env);
    let assignee = Address::generate(&env);

    let vesting_period = 86400u64; // 24 hours
    let (_, _, vault_id) = setup_vesting_vault(
        &env,
        &contract_id,
        &client,
        &token_contract,
        &creator,
        &assignee,
        500,
        vesting_period,
    );

    // Check remaining time
    let remaining = client.get_remaining_vesting_time(&vault_id);
    assert_eq!(remaining, vesting_period);

    // Check if vesting is complete
    assert!(!client.is_vesting_complete(&vault_id));

    // Advance time halfway
    env.ledger().set_timestamp(env.ledger().timestamp() + vesting_period / 2);

    let remaining = client.get_remaining_vesting_time(&vault_id);
    assert_eq!(remaining, vesting_period / 2);

    // Advance time past vesting
    env.ledger().set_timestamp(env.ledger().timestamp() + vesting_period / 2 + 1);

    assert!(client.is_vesting_complete(&vault_id));
    assert_eq!(client.get_remaining_vesting_time(&vault_id), 0);
}

// ── Test 9: Admin can release vault ───────────────────────────────────────

#[test]
fn test_admin_can_release_vault() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, contract_id, admin, token_contract, _) =
        setup_initialized_contract(&env, 100);

    let creator = Address::generate(&env);
    let assignee = Address::generate(&env);

    let vesting_period = 86400u64; // 24 hours
    let (_, _, vault_id) = setup_vesting_vault(
        &env,
        &contract_id,
        &client,
        &token_contract,
        &creator,
        &assignee,
        500,
        vesting_period,
    );

    // Advance time past vesting period
    env.ledger().set_timestamp(env.ledger().timestamp() + vesting_period + 1);

    // Admin releases funds
    let amount = client.release_vesting_vault(&admin, &vault_id);
    assert_eq!(amount, 500);

    // Verify funds transferred
    let token = soroban_sdk::token::Client::new(&env, &token_contract);
    assert_eq!(token.balance(&assignee), 500);
    assert_eq!(token.balance(&contract_id), 0);
}

// ── Test 10: Cannot create vault for non-approved milestone ────────────────

#[test]
fn test_cannot_create_vault_for_non_approved_milestone() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, contract_id, _, token_contract, _) =
        setup_initialized_contract(&env, 100);

    let creator = Address::generate(&env);
    let assignee = Address::generate(&env);

    let token_admin = StellarAssetClient::new(&env, &token_contract);
    token_admin.mint(&creator, &1000);

    let mut tags = Vec::new(&env);
    tags.push_back(Symbol::new(&env, "test"));

    let task_id = client.create_task(
        &creator,
        &Symbol::new(&env, "Test_Task"),
        &Symbol::new(&env, "Task_with_milestone"),
        &500,
        &tags,
    );

    client.assign_task(&assignee, &task_id);

    // Create milestone but don't approve it
    let e = env.clone();
    let ms_title = Symbol::new(&env, "Milestone_1");
    env.as_contract(&contract_id, move || {
        crate::escrow::create_milestone(e, task_id, ms_title, 500, None);
    });

    // Try to create vault without approving milestone - should fail
    let vesting_period = 86400u64;
    let result = client.try_create_vesting_vault(
        &creator,
        &task_id,
        &1,
        &assignee,
        &500,
        &token_contract,
        &vesting_period,
    );
    assert!(result.is_err(), "cannot create vault for non-approved milestone");
}

// ── Test 11: Cannot create duplicate vault for same milestone ──────────────

#[test]
fn test_cannot_create_duplicate_vault() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, contract_id, _, token_contract, _) =
        setup_initialized_contract(&env, 100);

    let creator = Address::generate(&env);
    let assignee = Address::generate(&env);

    let vesting_period = 86400u64;
    let (task_id, milestone_id, _) = setup_vesting_vault(
        &env,
        &contract_id,
        &client,
        &token_contract,
        &creator,
        &assignee,
        500,
        vesting_period,
    );

    // Try to create duplicate vault - should fail
    let result = client.try_create_vesting_vault(
        &creator,
        &task_id,
        &milestone_id,
        &assignee,
        &500,
        &token_contract,
        &vesting_period,
    );
    assert!(result.is_err(), "cannot create duplicate vault for same milestone");
}

// ── Test 12: Multiple vaults for different milestones ──────────────────────

#[test]
fn test_multiple_vaults_different_milestones() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, contract_id, _, token_contract, _) =
        setup_initialized_contract(&env, 100);

    let creator = Address::generate(&env);
    let assignee = Address::generate(&env);

    let token_admin = StellarAssetClient::new(&env, &token_contract);
    token_admin.mint(&creator, &2000);

    let mut tags = Vec::new(&env);
    tags.push_back(Symbol::new(&env, "test"));

    let task_id = client.create_task(
        &creator,
        &Symbol::new(&env, "Multi_Milestone_Task"),
        &Symbol::new(&env, "Task_with_multiple_milestones"),
        &1200,
        &tags,
    );

    client.assign_task(&assignee, &task_id);

    // Create two milestones
    let e = env.clone();
    let ms1_title = Symbol::new(&env, "Milestone_1");
    let ms2_title = Symbol::new(&env, "Milestone_2");
    env.as_contract(&contract_id, move || {
        crate::escrow::create_milestone(e.clone(), task_id, ms1_title, 500, None);
        crate::escrow::create_milestone(e, task_id, ms2_title, 700, None);
    });

    // Submit and approve both milestones with vesting
    client.submit_milestone(
        &assignee,
        &task_id,
        &1,
        &Symbol::new(&env, "https___github_com_pr_1"),
    );

    let vesting_period = 86400u64; // 24 hours
    let vault_id_1 = client.approve_milestone_with_vesting(
        &creator,
        &task_id,
        &1,
        &vesting_period,
        &None,
    );

    client.submit_milestone(
        &assignee,
        &task_id,
        &2,
        &Symbol::new(&env, "https___github_com_pr_2"),
    );

    let vault_id_2 = client.approve_milestone_with_vesting(
        &creator,
        &task_id,
        &2,
        &vesting_period,
        &None,
    );

    assert_eq!(vault_id_1, 1);
    assert_eq!(vault_id_2, 2);

    // Verify task vaults
    let task_vaults = client.get_task_vesting_vaults(&task_id);
    assert_eq!(task_vaults.len(), 2);

    // Verify beneficiary vaults
    let beneficiary_vaults = client.get_beneficiary_vesting_vaults(&assignee);
    assert_eq!(beneficiary_vaults.len(), 2);
}
