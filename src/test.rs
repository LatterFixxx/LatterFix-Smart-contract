#![cfg(test)]
#![allow(deprecated)]

use soroban_sdk::unwrap::UnwrapOptimized;
use crate::{TaskManagerContract, TaskManagerContractClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::testutils::Ledger as _;
use soroban_sdk::token::StellarAssetClient;
use soroban_sdk::xdr::ToXdr;
use soroban_sdk::{Address, BytesN, Env, Symbol, Vec};

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

// ── Test 1: Contract initialization ────────────────────────────────────────

#[test]
fn test_initialization() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, admin, token_contract, fee_recipient) = setup_initialized_contract(&env, 100);

    // Double-initialization must fail
    let res = client.try_initialize(&admin, &100, &token_contract, &fee_recipient);
    assert!(res.is_err(), "re-initialization should be rejected");
}

// ── Test 2: Full task lifecycle with fee assertion ─────────────────────────

#[test]
fn test_create_and_complete_task_flow() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, contract_id, _, token_contract, fee_recipient) =
        setup_initialized_contract(&env, 250); // 2.5% fee

    let creator = Address::generate(&env);
    let assignee = Address::generate(&env);

    StellarAssetClient::new(&env, &token_contract).mint(&creator, &1000);
    let token = soroban_sdk::token::Client::new(&env, &token_contract);
    assert_eq!(token.balance(&creator), 1000);

    let mut tags = Vec::new(&env);
    tags.push_back(Symbol::new(&env, "rust"));

    let task_id = client.create_task(
        &creator,
        &Symbol::new(&env, "Test_Task"),
        &Symbol::new(&env, "Task_Description"),
        &1000,
        &tags,
    );
    assert_eq!(task_id, 1);
    assert_eq!(token.balance(&creator), 0);
    assert_eq!(token.balance(&contract_id), 1000);

    client.assign_task(&assignee, &task_id);
    client.submit_work(
        &assignee,
        &task_id,
        &Symbol::new(
            &env,
            "https://github.com/LatterFixxx/LatterFix-Smart-contract",
        ),
    );
    client.complete_task(&creator, &task_id);

    // 2.5% of 1000 = 25 fee; assignee receives 975
    assert_eq!(token.balance(&fee_recipient), 25);
    assert_eq!(token.balance(&assignee), 975);
    assert_eq!(token.balance(&contract_id), 0);
}

// ── Test 3: Cancel refunds full amount ────────────────────────────────────

#[test]
fn test_cancel_task_refund() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, contract_id, _, token_contract, _) = setup_initialized_contract(&env, 100);

    let creator = Address::generate(&env);
    StellarAssetClient::new(&env, &token_contract).mint(&creator, &500);

    let task_id = client.create_task(
        &creator,
        &Symbol::new(&env, "Cancel_Task"),
        &Symbol::new(&env, "Will_cancel_this"),
        &500,
        &Vec::new(&env),
    );

    let token = soroban_sdk::token::Client::new(&env, &token_contract);
    assert_eq!(token.balance(&creator), 0);
    assert_eq!(token.balance(&contract_id), 500);

    client.cancel_task(&creator, &task_id);
    assert_eq!(
        token.balance(&creator),
        500,
        "creator must be fully refunded"
    );
    assert_eq!(token.balance(&contract_id), 0);
}

// ── Test 4: Dispute and 50/50 split resolution ────────────────────────────

#[test]
fn test_dispute_and_resolution() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, contract_id, admin, token_contract, _) = setup_initialized_contract(&env, 100);

    let creator = Address::generate(&env);
    let assignee = Address::generate(&env);
    StellarAssetClient::new(&env, &token_contract).mint(&creator, &1000);

    let task_id = client.create_task(
        &creator,
        &Symbol::new(&env, "Dispute_Task"),
        &Symbol::new(&env, "Dispute_test"),
        &1000,
        &Vec::new(&env),
    );
    client.assign_task(&assignee, &task_id);
    client.dispute_task(&creator, &task_id);
    client.resolve_dispute(&admin, &task_id, &500, &500);

    let token = soroban_sdk::token::Client::new(&env, &token_contract);
    assert_eq!(token.balance(&creator), 500, "creator gets 50%");
    assert_eq!(token.balance(&assignee), 500, "assignee gets 50%");
    assert_eq!(token.balance(&contract_id), 0);
}

// ── Test 5: User profile lifecycle ────────────────────────────────────────

#[test]
fn test_user_profile_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, admin, _, _) = setup_initialized_contract(&env, 100);

    let user = Address::generate(&env);
    let username = Symbol::new(&env, "john_doe");
    let bio = Symbol::new(&env, "Rust_developer");

    client.create_profile(&user, &username, &bio);

    let profile = client
        .get_profile(&user)
        .expect("profile must exist after creation");
    assert_eq!(profile.address, user);
    assert_eq!(profile.username, username);
    assert_eq!(profile.reputation, 100, "starting reputation is 100");
    assert_eq!(profile.completed_tasks, 0);
    assert_eq!(profile.bio, bio);

    let new_bio = Symbol::new(&env, "Soroban_developer");
    client.update_bio(&user, &new_bio);
    assert_eq!(client.get_profile(&user).unwrap_optimized().bio, new_bio);

    client.reward_contribution(&admin, &user, &25);
    let updated = client.get_profile(&user).unwrap_optimized();
    assert_eq!(updated.reputation, 125, "reputation must increase by 25");
    assert_eq!(updated.completed_tasks, 1);
}

// ── Test 6: Dispute resolved fully in favour of assignee ─────────────────

#[test]
fn test_dispute_full_assignee_payout() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, contract_id, admin, token_contract, _) = setup_initialized_contract(&env, 0); // 0% fee for clean assertions

    let creator = Address::generate(&env);
    let assignee = Address::generate(&env);
    StellarAssetClient::new(&env, &token_contract).mint(&creator, &800);

    let task_id = client.create_task(
        &creator,
        &Symbol::new(&env, "Full_Assignee_Payout"),
        &Symbol::new(&env, "Admin_rules_in_contributor_favou"),
        &800,
        &Vec::new(&env),
    );
    client.assign_task(&assignee, &task_id);
    client.dispute_task(&creator, &task_id);
    // Admin awards 100% to assignee
    client.resolve_dispute(&admin, &task_id, &0, &800);

    let token = soroban_sdk::token::Client::new(&env, &token_contract);
    assert_eq!(token.balance(&assignee), 800, "assignee gets full payout");
    assert_eq!(token.balance(&creator), 0);
    assert_eq!(token.balance(&contract_id), 0);
}

// ── Test 7: Multiple tasks share the same contract instance ───────────────

#[test]
fn test_multiple_concurrent_tasks() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, contract_id, _, token_contract, fee_recipient) =
        setup_initialized_contract(&env, 100); // 1% fee

    let creator = Address::generate(&env);
    let a1 = Address::generate(&env);
    let a2 = Address::generate(&env);

    StellarAssetClient::new(&env, &token_contract).mint(&creator, &3000);

    let mut tags = Vec::new(&env);
    tags.push_back(Symbol::new(&env, "frontend"));

    let t1 = client.create_task(
        &creator,
        &Symbol::new(&env, "Task_Alpha"),
        &Symbol::new(&env, "First_concurrent_task"),
        &1000,
        &tags,
    );
    let t2 = client.create_task(
        &creator,
        &Symbol::new(&env, "Task_Beta"),
        &Symbol::new(&env, "Second_concurrent_task"),
        &2000,
        &tags,
    );

    let token = soroban_sdk::token::Client::new(&env, &token_contract);
    assert_eq!(
        token.balance(&contract_id),
        3000,
        "both task rewards locked"
    );
    assert_eq!(token.balance(&creator), 0);

    // Complete task 1 → a1
    client.assign_task(&a1, &t1);
    client.submit_work(&a1, &t1, &Symbol::new(&env, "https___github_com_pr_1"));
    client.complete_task(&creator, &t1);

    // Complete task 2 → a2
    client.assign_task(&a2, &t2);
    client.submit_work(&a2, &t2, &Symbol::new(&env, "https___github_com_pr_2"));
    client.complete_task(&creator, &t2);

    // 1% of 1000 = 10, 1% of 2000 = 20 → fee_recipient gets 30
    assert_eq!(token.balance(&fee_recipient), 30, "combined fees correct");
    assert_eq!(token.balance(&a1), 990);
    assert_eq!(token.balance(&a2), 1980);
    assert_eq!(token.balance(&contract_id), 0);
}

// ── Test 8: Cannot assign to an already-assigned task ────────────────────

#[test]
fn test_cannot_double_assign() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, _, token_contract, _) = setup_initialized_contract(&env, 100);

    let creator = Address::generate(&env);
    let a1 = Address::generate(&env);
    let a2 = Address::generate(&env);

    StellarAssetClient::new(&env, &token_contract).mint(&creator, &500);

    let task_id = client.create_task(
        &creator,
        &Symbol::new(&env, "Single_Assign_Task"),
        &Symbol::new(&env, "Only_one_assignee_allowed"),
        &500,
        &Vec::new(&env),
    );

    client.assign_task(&a1, &task_id);
    // Second assignment to same task must fail
    let result = client.try_assign_task(&a2, &task_id);
    assert!(
        result.is_err(),
        "double-assigning a task should be rejected"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// TWAP Oracle Module Tests
// ═══════════════════════════════════════════════════════════════════════════

use crate::twap_oracle::*;

// ── Test 1: TWAP Configuration Initialization ──────────────────────────────

#[test]
fn test_twap_config_initialization() {
    let env = Env::default();
    let contract_id = env.register_contract(None, TaskManagerContract);
    let e = env.clone();
    env.as_contract(&contract_id, move || {
        let primary_pool = Symbol::new(&e, "primary_pool");
        let secondary_oracle = Some(Symbol::new(&e, "fallback_oracle"));
        
        initialize_twap_config(
            e.clone(),
            primary_pool.clone(),
            secondary_oracle.clone(),
            3,
            500,
            3600,
            10_000_000,
        );
        
        let config = get_twap_config(e);
        assert_eq!(config.primary_pool, primary_pool);
        assert_eq!(config.secondary_oracle, secondary_oracle);
        assert_eq!(config.min_observation_count, 3);
        assert_eq!(config.max_deviation_bps, 500);
        assert_eq!(config.observation_window_secs, 3600);
        assert_eq!(config.min_liquidity_threshold, 10_000_000);
    });
}

// ── Test 2: Record and Retrieve Price Observations ───────────────────────

#[test]
fn test_record_price_observations() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, contract_id, admin, _, _) = setup_initialized_contract(&env, 100);

    let usdc_admin = Address::generate(&env);
    let usdc = env.register_stellar_asset_contract(usdc_admin);
    let eurt_admin = Address::generate(&env);
    let eurt = env.register_stellar_asset_contract(eurt_admin);

    client.add_supported_token(&admin, &usdc);
    client.add_supported_token(&admin, &eurt);

    let worker = Address::generate(&env);
    StellarAssetClient::new(&env, &usdc).mint(&worker, &300);
    StellarAssetClient::new(&env, &eurt).mint(&worker, &200);

    client.deposit_to_vault(&worker, &usdc, &300);
    client.deposit_to_vault(&worker, &eurt, &200);

    client.claim_from_vault(&worker, &usdc, &300);

    assert_eq!(client.get_depositor_vault_balance(&worker, &usdc), 0);
    assert_eq!(
        client.get_depositor_vault_balance(&worker, &eurt),
        200,
    );
    assert_eq!(client.get_token_vault_balance(&usdc), 0);
    assert_eq!(client.get_token_vault_balance(&eurt), 200);

    let usdc_token = soroban_sdk::token::Client::new(&env, &usdc);
    assert_eq!(usdc_token.balance(&worker), 300);

    let e = env.clone();
    env.as_contract(&contract_id, move || {
        let asset_pair = Symbol::new(&e, "USDC_EUR");

        initialize_twap_config(
            e.clone(),
            Symbol::new(&e, "pool1"),
            None,
            3,
            500,
            3600,
            10_000_000,
        );

        record_price_observation(
            e.clone(),
            asset_pair.clone(),
            1_000_000_000,
            1_100_000_000,
            1_000,
            100,
        );

        record_price_observation(
            e.clone(),
            asset_pair.clone(),
            2_100_000_000,
            1_100_000_000,
            2_000,
            101,
        );

        let last_obs = get_last_twap(e.clone(), asset_pair.clone());
        assert!(last_obs.is_some());
        let last = last_obs.unwrap_optimized();
        assert_eq!(last.timestamp, 2_000);
        assert_eq!(last.price, 1_100_000_000);
    });
}

// ── Test 3: Multi-Period TWAP Accumulation ─────────────────────────────────

#[test]
fn test_multi_period_twap_accumulation() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, contract_id, _, _, _) = setup_initialized_contract(&env, 100);

    let orgusd_admin = Address::generate(&env);
    let orgusd = env.register_stellar_asset_contract(orgusd_admin);

    let depositor = Address::generate(&env);
    StellarAssetClient::new(&env, &orgusd).mint(&depositor, &100);

    let result = client.try_deposit_to_vault(&depositor, &orgusd, &100);
    assert!(result.is_err(), "depositing an unsupported token must be rejected");

    let e = env.clone();
    env.as_contract(&contract_id, move || {
        e.ledger().set_timestamp(1_005_000);

        let asset_pair = Symbol::new(&e, "USDC_EUR");

        initialize_twap_config(
            e.clone(),
            Symbol::new(&e, "pool1"),
            None,
            2,
            500,
            36000,
            10_000_000,
        );

        update_pool_liquidity(e.clone(), asset_pair.clone(), 50_000_000);

        let base_time = 1_000_000u64;

        record_price_observation(
            e.clone(),
            asset_pair.clone(),
            1_000_000_000_000,
            1_000_000_000,
            base_time,
            100,
        );

        record_price_observation(
            e.clone(),
            asset_pair.clone(),
            2_050_000_000_000,
            1_050_000_000,
            base_time + 1000,
            101,
        );

        record_price_observation(
            e.clone(),
            asset_pair.clone(),
            3_070_000_000_000,
            1_020_000_000,
            base_time + 2000,
            102,
        );

        let twap_result = calculate_twap(e.clone(), asset_pair.clone());

        assert_eq!(twap_result.observation_count, 3);
        assert!(!twap_result.used_fallback);
        assert_eq!(twap_result.oldest_timestamp, base_time);
        assert_eq!(twap_result.newest_timestamp, base_time + 2000);
        assert!(twap_result.price > 900_000_000 && twap_result.price < 1_200_000_000);
    });
}

// ── Test 4: Outlier Price Filter ───────────────────────────────────────────

#[test]
fn test_outlier_price_filter() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, contract_id, admin, _, _) = setup_initialized_contract(&env, 100);

    let usdc_admin = Address::generate(&env);
    let usdc = env.register_stellar_asset_contract(usdc_admin);
    client.add_supported_token(&admin, &usdc);

    let depositor = Address::generate(&env);
    StellarAssetClient::new(&env, &usdc).mint(&depositor, &50);
    client.deposit_to_vault(&depositor, &usdc, &50);

    let result = client.try_claim_from_vault(&depositor, &usdc, &51);
    assert!(result.is_err(), "claiming more than the deposited balance must be rejected");

    let e = env.clone();
    env.as_contract(&contract_id, move || {
        e.ledger().set_timestamp(1_005_000);

        let asset_pair = Symbol::new(&e, "USDC_USD");

        initialize_twap_config(
            e.clone(),
            Symbol::new(&e, "pool1"),
            None,
            3,
            500,
            36000,
            10_000_000,
        );

        update_pool_liquidity(e.clone(), asset_pair.clone(), 50_000_000);

        let base_time = 1_000_000u64;

        record_price_observation(
            e.clone(),
            asset_pair.clone(),
            1_000_000_000,
            1_000_000_000,
            base_time,
            100,
        );

        record_price_observation(
            e.clone(),
            asset_pair.clone(),
            2_010_000_000,
            1_010_000_000,
            base_time + 1000,
            101,
        );

        record_price_observation(
            e.clone(),
            asset_pair.clone(),
            3_210_000_000,
            1_200_000_000,
            base_time + 2000,
            102,
        );

        record_price_observation(
            e.clone(),
            asset_pair.clone(),
            4_220_000_000,
            1_020_000_000,
            base_time + 3000,
            103,
        );

        let twap_result = calculate_twap(e.clone(), asset_pair.clone());

        assert_eq!(twap_result.observation_count, 3);
        assert!(!twap_result.used_fallback);
        assert!(twap_result.avg_deviation_bps < 200);
    });
}

// ── Test 5: Fallback Oracle on Low Liquidity ───────────────────────────────

#[test]
fn test_fallback_oracle_low_liquidity() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, contract_id, admin, _, _) = setup_initialized_contract(&env, 100);

    let usdc_admin = Address::generate(&env);
    let usdc = env.register_stellar_asset_contract(usdc_admin);
    client.add_supported_token(&admin, &usdc);
    assert!(client.is_token_supported(&usdc));

    client.remove_supported_token(&admin, &usdc);
    assert!(!client.is_token_supported(&usdc));

    let depositor = Address::generate(&env);
    StellarAssetClient::new(&env, &usdc).mint(&depositor, &10);
    let result = client.try_deposit_to_vault(&depositor, &usdc, &10);
    assert!(result.is_err(), "deposits must be rejected after a token is removed");

    let e = env.clone();
    env.as_contract(&contract_id, move || {
        e.ledger().set_timestamp(1_005_000);

        let asset_pair = Symbol::new(&e, "USDC_JPY");

        let fallback_oracle = Symbol::new(&e, "fallback");

        initialize_twap_config(
            e.clone(),
            Symbol::new(&e, "pool1"),
            Some(fallback_oracle),
            2,
            500,
            3600,
            1_000_000_000,
        );

        update_pool_liquidity(e.clone(), asset_pair.clone(), 100_000_000);

        let base_time = 1_000_000u64;
        record_price_observation(
            e.clone(),
            asset_pair.clone(),
            1_000_000_000,
            980_000_000,
            base_time,
            100,
        );

        set_fallback_price(
            e.clone(),
            asset_pair.clone(),
            975_000_000,
            base_time,
        );

        let twap_result = calculate_twap(e.clone(), asset_pair.clone());

        assert!(twap_result.used_fallback);
        assert_eq!(twap_result.price, 975_000_000);
        assert_eq!(twap_result.observation_count, 1);
    });
}

// ── Test 6: Fallback on Insufficient Observations ─────────────────────────

#[test]
fn test_fallback_on_insufficient_observations() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, contract_id, admin, token_contract, _) = setup_initialized_contract(&env, 100);

    let token_admin_client = soroban_sdk::token::StellarAssetClient::new(&env, &token_contract);
    let _token_client = soroban_sdk::token::Client::new(&env, &token_contract);

    let claimant1 = Address::generate(&env);
    let _claimant2 = Address::generate(&env);
    let amount1: i128 = 1000;
    let _amount2: i128 = 2000;

    let depositor = Address::generate(&env);
    token_admin_client.mint(&depositor, &5000);

    client.add_supported_token(&admin, &token_contract);
    client.deposit_to_vault(&depositor, &token_contract, &5000);

    let leaf1_data = (claimant1.clone(), token_contract.clone(), amount1).to_xdr(&env);
    let _leaf1: BytesN<32> = env.crypto().sha256(&leaf1_data).into();

    let e = env.clone();
    env.as_contract(&contract_id, move || {
        e.ledger().set_timestamp(1_005_000);

        let asset_pair = Symbol::new(&e, "USDC_GBP");

        let fallback_oracle = Symbol::new(&e, "fallback");

        initialize_twap_config(
            e.clone(),
            Symbol::new(&e, "pool1"),
            Some(fallback_oracle),
            3,
            500,
            3600,
            10_000_000,
        );

        update_pool_liquidity(e.clone(), asset_pair.clone(), 50_000_000);

        let base_time = 1_000_000u64;

        record_price_observation(
            e.clone(),
            asset_pair.clone(),
            1_000_000_000,
            1_000_000_000,
            base_time,
            100,
        );

        set_fallback_price(
            e.clone(),
            asset_pair.clone(),
            950_000_000,
            base_time,
        );

        let twap_result = calculate_twap(e.clone(), asset_pair.clone());

        assert!(twap_result.used_fallback);
        assert_eq!(twap_result.price, 950_000_000);
    });
}

// ── Test 7: Pool Liquidity Status Check ────────────────────────────────────

#[test]
fn test_pool_liquidity_checks() {
    let env = Env::default();
    let contract_id = env.register_contract(None, TaskManagerContract);
    let e = env.clone();
    env.as_contract(&contract_id, move || {
        let asset_pair = Symbol::new(&e, "USDC_CHF");
        
        initialize_twap_config(
            e.clone(),
            Symbol::new(&e, "pool1"),
            None,
            2,
            500,
            3600,
            20_000_000,
        );
        
        assert!(!is_pool_liquid_enough(
            e.clone(),
            asset_pair.clone()
        ));
        
        update_pool_liquidity(e.clone(), asset_pair.clone(), 25_000_000);
        assert!(is_pool_liquid_enough(
            e.clone(),
            asset_pair.clone()
        ));
        
        assert_eq!(get_pool_liquidity(e.clone(), asset_pair.clone()), 25_000_000);
        
        update_pool_liquidity(e.clone(), asset_pair.clone(), 15_000_000);
        assert!(!is_pool_liquid_enough(e.clone(), asset_pair.clone()));
    });
}

// ── Test 8: Observation Pruning ────────────────────────────────────────────

#[test]
fn test_prune_old_observations() {
    let env = Env::default();
    let contract_id = env.register_contract(None, TaskManagerContract);
    let e = env.clone();
    env.as_contract(&contract_id, move || {
        e.ledger().set_timestamp(100_000);

        let asset_pair = Symbol::new(&e, "USDC_CAD");

        initialize_twap_config(
            e.clone(),
            Symbol::new(&e, "pool1"),
            None,
            2,
            500,
            3600,
            10_000_000,
        );

        let base_time = 100_000u64;

        record_price_observation(
            e.clone(),
            asset_pair.clone(),
            1_000_000_000,
            1_000_000_000,
            base_time - 10_000,
            100,
        );

        record_price_observation(
            e.clone(),
            asset_pair.clone(),
            2_010_000_000,
            1_010_000_000,
            base_time - 5_000,
            101,
        );

        record_price_observation(
            e.clone(),
            asset_pair.clone(),
            3_020_000_000,
            1_020_000_000,
            base_time,
            102,
        );

        let pruned_count = prune_old_observations(e.clone(), asset_pair.clone(), 6_000);

        assert_eq!(pruned_count, 1);
    });
}

// ── Test 9: Proper Window Filtering ───────────────────────────────────────

#[test]
fn test_twap_observation_window_filtering() {
    let env = Env::default();
    let contract_id = env.register_contract(None, TaskManagerContract);
    let e = env.clone();
    env.as_contract(&contract_id, move || {
        e.ledger().set_timestamp(100_000);

        let asset_pair = Symbol::new(&e, "USDC_AUD");

        initialize_twap_config(
            e.clone(),
            Symbol::new(&e, "pool1"),
            None,
            2,
            500,
            1000,
            10_000_000,
        );

        update_pool_liquidity(e.clone(), asset_pair.clone(), 50_000_000);

        let base_time = 100_000u64;

        record_price_observation(
            e.clone(),
            asset_pair.clone(),
            1_000_000_000,
            1_000_000_000,
            base_time - 2000,
            100,
        );

        record_price_observation(
            e.clone(),
            asset_pair.clone(),
            2_010_000_000,
            1_010_000_000,
            base_time - 500,
            101,
        );

        record_price_observation(
            e.clone(),
            asset_pair.clone(),
            3_020_000_000,
            1_020_000_000,
            base_time,
            102,
        );

        let twap_result = calculate_twap(e.clone(), asset_pair.clone());

        assert_eq!(twap_result.observation_count, 2);
        assert_eq!(twap_result.oldest_timestamp, base_time - 500);
    });
}

// ── Test 10: TWAP with Edge Case Prices ────────────────────────────────────

#[test]
fn test_twap_edge_case_prices() {
    let env = Env::default();
    let contract_id = env.register_contract(None, TaskManagerContract);
    let e = env.clone();
    env.as_contract(&contract_id, move || {
        e.ledger().set_timestamp(1_005_000);

        let asset_pair = Symbol::new(&e, "USDC_NZD");

        initialize_twap_config(
            e.clone(),
            Symbol::new(&e, "pool1"),
            None,
            2,
            1000,
            36000,
            10_000_000,
        );

        update_pool_liquidity(e.clone(), asset_pair.clone(), 50_000_000);

        let base_time = 1_000_000u64;

        record_price_observation(
            e.clone(),
            asset_pair.clone(),
            100,
            100,
            base_time,
            100,
        );

        record_price_observation(
            e.clone(),
            asset_pair.clone(),
            200,
            100,
            base_time + 1000,
            101,
        );

        let twap_result = calculate_twap(e.clone(), asset_pair.clone());
        assert!(twap_result.price >= 0);
        assert_eq!(twap_result.observation_count, 2);
    });
}

// ═══════════════════════════════════════════════════════════════════════════
// ZKP Identity Attestation Module Tests
// ═══════════════════════════════════════════════════════════════════════════

use crate::zkp_attestation::*;
use soroban_sdk::Bytes;

// ── Fixtures ───────────────────────────────────────────────────────────────

fn zk_proof(env: &Env) -> Groth16Proof {
    Groth16Proof {
        a: Bytes::from_array(env, &[1u8; 96]),
        b: Bytes::from_array(env, &[2u8; 192]),
        c: Bytes::from_array(env, &[3u8; 96]),
    }
}

fn zk_ic(env: &Env, n: u32) -> Vec<Bytes> {
    let mut ic = Vec::new(env);
    for i in 0..(n + 1) {
        ic.push_back(Bytes::from_array(env, &[(i as u8) + 1; 96]));
    }
    ic
}

fn zk_signals(env: &Env, n: u32) -> Vec<BytesN<32>> {
    let mut signals = Vec::new(env);
    for i in 0..n {
        signals.push_back(BytesN::from_array(env, &[(i as u8) + 10; 32]));
    }
    signals
}

fn zk_setup(env: &Env) -> (Address, Symbol) {
    let admin = Address::generate(env);
    initialize(env.clone(), admin.clone());
    let circuit_id = Symbol::new(&env, "kyc_tier_1");
    register_verification_key(
        env.clone(),
        admin.clone(),
        circuit_id.clone(),
        Symbol::new(&env, "BLS12_381"),
        Bytes::from_array(env, &[9u8; 32]),
        Bytes::from_array(env, &[8u8; 192]),
        Bytes::from_array(env, &[7u8; 192]),
        zk_ic(env, 2),
    );
    (admin, circuit_id)
}

fn zk_attestation(
    env: &Env,
    circuit_id: &Symbol,
    subject: &Address,
    nullifier: BytesN<32>,
) -> IdentityAttestation {
    let signals = zk_signals(env, 2);
    let commitment =
        compute_attestation_commitment(env.clone(), nullifier.clone(), signals.clone());
    IdentityAttestation {
        circuit_id: circuit_id.clone(),
        subject: subject.clone(),
        proof: zk_proof(env),
        public_signals: signals,
        nullifier,
        attestation_commitment: commitment,
    }
}

// ── Test: a valid attestation is accepted and recorded ──────────────────────

#[test]
fn test_zkp_valid_attestation() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, TaskManagerContract);

    env.as_contract(&contract_id, || {
        let (_admin, circuit_id) = zk_setup(&env);
        let subject = Address::generate(&env);
        let nullifier = BytesN::from_array(&env, &[42u8; 32]);

        assert!(!is_nullifier_used(env.clone(), nullifier.clone()));

        let attestation = zk_attestation(&env, &circuit_id, &subject, nullifier.clone());
        let receipt = verify_attestation(env.clone(), attestation).unwrap_optimized();

        assert_eq!(receipt.subject, subject);
        assert_eq!(receipt.circuit_id, circuit_id);
        assert!(is_nullifier_used(env.clone(), nullifier.clone()));
        assert_eq!(attestation_count(env.clone()), 1);
        assert!(get_attestation(env.clone(), nullifier).is_some());
    });
}

// ── Test: replaying a spent nullifier is rejected ───────────────────────────

#[test]
fn test_zkp_rejects_replayed_nullifier() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, TaskManagerContract);

    env.as_contract(&contract_id, || {
        let (_admin, circuit_id) = zk_setup(&env);
        let subject_a = Address::generate(&env);
        let subject_b = Address::generate(&env);
        let nullifier = BytesN::from_array(&env, &[7u8; 32]);

        let first = zk_attestation(&env, &circuit_id, &subject_a, nullifier.clone());
        assert!(verify_attestation(env.clone(), first).is_ok());

        // Same nullifier a second time — even for a different subject with an
        // otherwise valid payload: replay protection keys on the nullifier.
        let replay = zk_attestation(&env, &circuit_id, &subject_b, nullifier);
        assert_eq!(
            verify_attestation(env.clone(), replay),
            Err(AttestationError::NullifierAlreadyUsed)
        );
        // The count must not have advanced.
        assert_eq!(attestation_count(env.clone()), 1);
    });
}

// ── Test: unregistered circuit is rejected ──────────────────────────────────

#[test]
fn test_zkp_rejects_unregistered_circuit() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, TaskManagerContract);

    env.as_contract(&contract_id, || {
        let (_admin, _circuit_id) = zk_setup(&env);
        let subject = Address::generate(&env);
        let nullifier = BytesN::from_array(&env, &[1u8; 32]);

        let unknown = Symbol::new(&env, "unknown_circuit");
        let attestation = zk_attestation(&env, &unknown, &subject, nullifier);
        assert_eq!(
            verify_attestation(env.clone(), attestation),
            Err(AttestationError::CircuitNotRegistered)
        );
    });
}

// ── Test: malformed proof (zero / wrong-length points) is rejected ──────────

#[test]
fn test_zkp_rejects_malformed_proof() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, TaskManagerContract);

    env.as_contract(&contract_id, || {
        let (_admin, circuit_id) = zk_setup(&env);
        let subject_a = Address::generate(&env);
        let subject_b = Address::generate(&env);

        // All-zero G1 point `A` — never a valid proof element.
        let nullifier_a = BytesN::from_array(&env, &[5u8; 32]);
        let mut attestation = zk_attestation(&env, &circuit_id, &subject_a, nullifier_a);
        attestation.proof.a = Bytes::from_array(&env, &[0u8; 96]);
        assert_eq!(
            verify_attestation(env.clone(), attestation),
            Err(AttestationError::MalformedProof)
        );

        // Wrong-length G2 point `B`.
        let nullifier_b = BytesN::from_array(&env, &[6u8; 32]);
        let mut bad_len = zk_attestation(&env, &circuit_id, &subject_b, nullifier_b);
        bad_len.proof.b = Bytes::from_array(&env, &[2u8; 64]);
        assert_eq!(
            verify_attestation(env.clone(), bad_len),
            Err(AttestationError::MalformedProof)
        );
    });
}

// ── Test: public-signal arity mismatch is rejected ──────────────────────────

#[test]
fn test_zkp_rejects_public_signal_mismatch() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, TaskManagerContract);

    env.as_contract(&contract_id, || {
        let (_admin, circuit_id) = zk_setup(&env); // VK expects 2 signals
        let subject = Address::generate(&env);
        let nullifier = BytesN::from_array(&env, &[9u8; 32]);

        // Present 3 signals against a 2-signal VK (IC arity is 3).
        let signals = zk_signals(&env, 3);
        let commitment =
            compute_attestation_commitment(env.clone(), nullifier.clone(), signals.clone());
        let attestation = IdentityAttestation {
            circuit_id,
            subject,
            proof: zk_proof(&env),
            public_signals: signals,
            nullifier,
            attestation_commitment: commitment,
        };
        assert_eq!(
            verify_attestation(env.clone(), attestation),
            Err(AttestationError::PublicSignalMismatch)
        );
    });
}

// ── Test: a tampered commitment is rejected ─────────────────────────────────

#[test]
fn test_zkp_rejects_commitment_mismatch() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, TaskManagerContract);

    env.as_contract(&contract_id, || {
        let (_admin, circuit_id) = zk_setup(&env);
        let subject = Address::generate(&env);
        let nullifier = BytesN::from_array(&env, &[3u8; 32]);

        let mut attestation = zk_attestation(&env, &circuit_id, &subject, nullifier);
        // Overwrite the correctly-derived commitment with a bogus one.
        attestation.attestation_commitment = BytesN::from_array(&env, &[0xAAu8; 32]);
        assert_eq!(
            verify_attestation(env.clone(), attestation),
            Err(AttestationError::CommitmentMismatch)
        );
    });
}

// ── Test: a valid proof cannot be lifted onto a different nullifier ──────────

#[test]
fn test_zkp_nullifier_binding_blocks_proof_lifting() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, TaskManagerContract);

    env.as_contract(&contract_id, || {
        let (_admin, circuit_id) = zk_setup(&env);
        let subject = Address::generate(&env);

        // Commitment is bound to nullifier A...
        let nullifier_a = BytesN::from_array(&env, &[0x11u8; 32]);
        let mut attestation = zk_attestation(&env, &circuit_id, &subject, nullifier_a);
        // ...but the attacker swaps in a fresh, unspent nullifier B without
        // recomputing the commitment, hoping to mint a new attestation.
        attestation.nullifier = BytesN::from_array(&env, &[0x22u8; 32]);
        assert_eq!(
            verify_attestation(env.clone(), attestation),
            Err(AttestationError::CommitmentMismatch)
        );
    });
}

// ── Test: only the admin may register verification keys ─────────────────────

#[test]
#[should_panic]
fn test_zkp_register_vk_requires_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, TaskManagerContract);

    env.as_contract(&contract_id, || {
        let (_admin, _circuit_id) = zk_setup(&env);
        let impostor = Address::generate(&env);
        register_verification_key(
            env.clone(),
            impostor,
            Symbol::new(&env, "kyc_tier_2"),
            Symbol::new(&env, "BLS12_381"),
            Bytes::from_array(&env, &[9u8; 32]),
            Bytes::from_array(&env, &[8u8; 192]),
            Bytes::from_array(&env, &[7u8; 192]),
            zk_ic(&env, 2),
        );
    });
}

// ── Test: the commitment derivation is deterministic ────────────────────────

#[test]
fn test_zkp_commitment_is_deterministic() {
    let env = Env::default();
    let nullifier = BytesN::from_array(&env, &[4u8; 32]);
    let signals = zk_signals(&env, 2);

    let first = compute_attestation_commitment(env.clone(), nullifier.clone(), signals.clone());
    let second = compute_attestation_commitment(env.clone(), nullifier, signals);
    assert_eq!(first, second);
}

// ── Test: Dispute split with 3-way fractional payout ───────────────────────

#[test]
fn test_dispute_split_three_way() {
    let env = Env::default();
    env.mock_all_auths();

    // 10% platform fee so fee math is obvious
    let (client, contract_id, _, token_contract, fee_recipient) =
        setup_initialized_contract(&env, 1000);

    let creator = Address::generate(&env);
    let a1 = Address::generate(&env);
    let a2 = Address::generate(&env);
    let _a3 = Address::generate(&env);

    StellarAssetClient::new(&env, &token_contract).mint(&creator, &1000);

    let task_id = client.create_task(
        &creator,
        &Symbol::new(&env, "Split_Task"),
        &Symbol::new(&env, "3_way_split"),
        &1000,
        &Vec::new(&env),
    );
    client.assign_task(&a1, &task_id);
    client.dispute_task(&creator, &task_id);

    // 40% / 30% / 30%
    let mut recipients = Vec::new(&env);
    recipients.push_back(creator.clone());
    recipients.push_back(a1.clone());
    recipients.push_back(a2.clone());

    let mut shares = Vec::new(&env);
    shares.push_back(4000u32);
    shares.push_back(3000u32);
    shares.push_back(3000u32);

    client.resolve_dispute_split(&task_id, &recipients, &shares);

    // fee = 1000 * 1000 / 10000 = 100
    // distributable = 900
    // creator: 900 * 4000 / 10000 = 360
    // a1:      900 * 3000 / 10000 = 270
    // a2:      900 * 3000 / 10000 = 270
    let token = soroban_sdk::token::Client::new(&env, &token_contract);
    assert_eq!(token.balance(&fee_recipient), 100, "platform fee");
    assert_eq!(token.balance(&creator), 360, "creator 40% share");
    assert_eq!(token.balance(&a1), 270, "a1 30% share");
    assert_eq!(token.balance(&a2), 270, "a2 30% share");
    assert_eq!(token.balance(&contract_id), 0, "escrow fully drained");
}

// ── Test: Dispute split with zero fee ──────────────────────────────────────

#[test]
fn test_dispute_split_zero_fee() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, contract_id, _, token_contract, _) =
        setup_initialized_contract(&env, 0); // 0% fee

    let creator = Address::generate(&env);
    let assignee = Address::generate(&env);
    let mediator = Address::generate(&env);

    StellarAssetClient::new(&env, &token_contract).mint(&creator, &800);

    let task_id = client.create_task(
        &creator,
        &Symbol::new(&env, "Zero_Fee_Split"),
        &Symbol::new(&env, "Split_with_no_fee"),
        &800,
        &Vec::new(&env),
    );
    client.assign_task(&assignee, &task_id);
    client.dispute_task(&creator, &task_id);

    // 25% / 25% / 50%
    let mut recipients = Vec::new(&env);
    recipients.push_back(creator.clone());
    recipients.push_back(assignee.clone());
    recipients.push_back(mediator.clone());

    let mut shares = Vec::new(&env);
    shares.push_back(2500u32);
    shares.push_back(2500u32);
    shares.push_back(5000u32);

    client.resolve_dispute_split(&task_id, &recipients, &shares);

    let token = soroban_sdk::token::Client::new(&env, &token_contract);
    assert_eq!(token.balance(&creator), 200, "creator 25%");
    assert_eq!(token.balance(&assignee), 200, "assignee 25%");
    assert_eq!(token.balance(&mediator), 400, "mediator 50%");
    assert_eq!(token.balance(&contract_id), 0);
}

// ── Test: Dispute split rejects when task not disputed ─────────────────────

#[test]
fn test_dispute_split_rejects_non_disputed() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, _, token_contract, _) = setup_initialized_contract(&env, 100);

    let creator = Address::generate(&env);
    let a1 = Address::generate(&env);

    StellarAssetClient::new(&env, &token_contract).mint(&creator, &100);

    let task_id = client.create_task(
        &creator,
        &Symbol::new(&env, "Normal_Task"),
        &Symbol::new(&env, "Not_disputed"),
        &100,
        &Vec::new(&env),
    );
    client.assign_task(&a1, &task_id);

    let mut recipients = Vec::new(&env);
    recipients.push_back(creator.clone());
    recipients.push_back(a1.clone());

    let mut shares = Vec::new(&env);
    shares.push_back(5000u32);
    shares.push_back(5000u32);

    let res = client.try_resolve_dispute_split(&task_id, &recipients, &shares);
    assert!(res.is_err(), "must reject when task is not disputed");
}

// ── Test: Dispute split rejects invalid share sum ──────────────────────────

#[test]
fn test_dispute_split_rejects_invalid_shares() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, _admin, token_contract, _) = setup_initialized_contract(&env, 0);

    let creator = Address::generate(&env);
    let assignee = Address::generate(&env);

    StellarAssetClient::new(&env, &token_contract).mint(&creator, &500);

    let task_id = client.create_task(
        &creator,
        &Symbol::new(&env, "Bad_Shares"),
        &Symbol::new(&env, "shares_don_t_add_up"),
        &500,
        &Vec::new(&env),
    );
    client.assign_task(&assignee, &task_id);
    client.dispute_task(&creator, &task_id);

    // 30% + 30% = 60% — not 100%
    let mut recipients = Vec::new(&env);
    recipients.push_back(creator.clone());
    recipients.push_back(assignee.clone());

    let mut shares = Vec::new(&env);
    shares.push_back(3000u32);
    shares.push_back(3000u32);

    let res = client.try_resolve_dispute_split(&task_id, &recipients, &shares);
    assert!(res.is_err(), "must reject when shares don't sum to 10000");
}

// ── Test: Dispute split rejects mismatched lengths ─────────────────────────

#[test]
fn test_dispute_split_rejects_mismatched_lengths() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, _, token_contract, _) = setup_initialized_contract(&env, 0);

    let creator = Address::generate(&env);
    let assignee = Address::generate(&env);

    StellarAssetClient::new(&env, &token_contract).mint(&creator, &500);

    let task_id = client.create_task(
        &creator,
        &Symbol::new(&env, "Mismatch"),
        &Symbol::new(&env, "lengths_differ"),
        &500,
        &Vec::new(&env),
    );
    client.assign_task(&assignee, &task_id);
    client.dispute_task(&creator, &task_id);

    let mut recipients = Vec::new(&env);
    recipients.push_back(creator.clone());
    recipients.push_back(assignee.clone());

    let mut shares = Vec::new(&env);
    shares.push_back(10000u32); // only one share for two recipients

    let res = client.try_resolve_dispute_split(&task_id, &recipients, &shares);
    assert!(res.is_err(), "must reject when lengths mismatch");
}

// ── Test: Dispute split via multisig proposal ──────────────────────────────

#[test]
fn test_dispute_split_via_multisig_proposal() {
    use crate::multisig::{MultisigAction, MultisigProposalStatus};

    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, TaskManagerContract);
    let client = TaskManagerContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(token_admin.clone());
    let fee_recipient = Address::generate(&env);

    client.initialize(&admin, &200u32, &token, &fee_recipient); // 2% fee

    // Setup 2-of-3 multisig
    let s0 = Address::generate(&env);
    let s1 = Address::generate(&env);
    let s2 = Address::generate(&env);
    let mut signers = Vec::new(&env);
    signers.push_back(s0.clone());
    signers.push_back(s1.clone());
    signers.push_back(s2.clone());
    client.configure_multisig(&admin, &signers, &2u32, &None, &None);

    // Create task + dispute
    let creator = Address::generate(&env);
    let assignee = Address::generate(&env);

    StellarAssetClient::new(&env, &token).mint(&creator, &1000);

    let task_id = client.create_task(
        &creator,
        &Symbol::new(&env, "MS_Split_Task"),
        &Symbol::new(&env, "multisig_split"),
        &1000,
        &Vec::new(&env),
    );
    client.assign_task(&assignee, &task_id);
    client.dispute_task(&creator, &task_id);

    // Propose split: 60% creator, 40% assignee
    let mut recipients = Vec::new(&env);
    recipients.push_back(creator.clone());
    recipients.push_back(assignee.clone());

    let mut shares = Vec::new(&env);
    shares.push_back(6000u32);
    shares.push_back(4000u32);

    let desc = Symbol::new(&env, "resolve_60_40");
    let action = MultisigAction::ResolveDisputeSplit(task_id, recipients, shares);
    let proposal_id = client.multisig_propose(&s0, &desc, &action);
    assert_eq!(proposal_id, 1);

    // First approval — still pending
    let status = client.vote_proposal(&s0, &proposal_id);
    assert_eq!(status, MultisigProposalStatus::Pending);

    // Second approval triggers auto-execute
    let status = client.vote_proposal(&s1, &proposal_id);
    assert_eq!(status, MultisigProposalStatus::Executed);

    // fee = 1000 * 200 / 10000 = 20
    // distributable = 980
    // creator:  980 * 6000 / 10000 = 588
    // assignee: 980 * 4000 / 10000 = 392
    let token_client = soroban_sdk::token::Client::new(&env, &token);
    assert_eq!(token_client.balance(&fee_recipient), 20);
    assert_eq!(token_client.balance(&creator), 588);
    assert_eq!(token_client.balance(&assignee), 392);
    assert_eq!(token_client.balance(&contract_id), 0);
}
