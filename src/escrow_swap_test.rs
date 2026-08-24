#![cfg(test)]
#![allow(deprecated)]

// Tests for #083: funding task escrows by swapping an arbitrary input token
// into the contract's canonical stablecoin via `create_task_with_swap`,
// atomically and through the multi-hop swap router.

use crate::swap_router::SwapRoute;
use crate::swap_router_test::{MockOracle, MockOracleClient, MockPoolClient};
use crate::{Task, TaskManagerContract, TaskManagerContractClient, TaskStatus};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::token::StellarAssetClient;
use soroban_sdk::{symbol_short, Address, Env, Vec};

const ONE: i128 = 10_000_000; // oracle price scale, 10^ORACLE_PRICE_DECIMALS

fn setup(
    env: &Env,
) -> (
    TaskManagerContractClient<'static>,
    Address, // contract id
    Address, // admin
    Address, // escrow stablecoin (the contract's configured TokenContract)
    Address, // oracle
) {
    let contract_id = env.register_contract(None, TaskManagerContract);
    let client = TaskManagerContractClient::new(env, &contract_id);

    let admin = Address::generate(env);
    let token_admin = Address::generate(env);
    let token_contract = env.register_stellar_asset_contract(token_admin);
    let fee_recipient = Address::generate(env);

    client.initialize(&admin, &100u32, &token_contract, &fee_recipient);

    let oracle_id = env.register_contract(None, MockOracle);
    client.configure_swap_router(&admin, &oracle_id, &4u32, &500u32); // 5% default slippage

    (client, contract_id, admin, token_contract, oracle_id)
}

fn new_token(env: &Env) -> Address {
    let admin = Address::generate(env);
    env.register_stellar_asset_contract(admin)
}

fn new_pool(env: &Env, rate_bps: i128) -> Address {
    let pool_id = env.register_contract(None, crate::swap_router_test::MockPool);
    let pool_client = MockPoolClient::new(env, &pool_id);
    pool_client.init(&rate_bps);
    pool_id
}

#[test]
fn test_create_task_with_swap_single_hop_success() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, contract_id, _admin, stablecoin, oracle_id) = setup(&env);
    let oracle = MockOracleClient::new(&env, &oracle_id);

    let token_in = new_token(&env);
    oracle.set_price(&token_in, &ONE);
    oracle.set_price(&stablecoin, &ONE);

    let pool = new_pool(&env, 10_000); // 1:1 rate
    StellarAssetClient::new(&env, &stablecoin).mint(&pool, &10_000);

    let creator = Address::generate(&env);
    StellarAssetClient::new(&env, &token_in).mint(&creator, &1_000);

    let mut path = Vec::new(&env);
    path.push_back(token_in.clone());
    path.push_back(stablecoin.clone());
    let mut pools = Vec::new(&env);
    pools.push_back(pool);
    let route = SwapRoute { path, pools };

    let tags = Vec::new(&env);
    let task_id = client.create_task_with_swap(
        &creator,
        &symbol_short!("title"),
        &symbol_short!("desc"),
        &token_in,
        &1_000,
        &route,
        &None,
        &tags,
    );

    let task: Task = client.get_task(&task_id).unwrap();
    assert_eq!(task.reward, 1_000);
    assert!(matches!(task.status, TaskStatus::Open));
    assert_eq!(task.created_by, creator);

    let token_in_client = soroban_sdk::token::Client::new(&env, &token_in);
    assert_eq!(token_in_client.balance(&creator), 0, "input token fully pulled");

    let stablecoin_client = soroban_sdk::token::Client::new(&env, &stablecoin);
    assert_eq!(
        stablecoin_client.balance(&contract_id),
        1_000,
        "swapped stablecoin lands in the contract, funding the escrow lock"
    );

    let escrow_stats = client.get_escrow_stats();
    assert_eq!(escrow_stats.total_locked, 1_000);
}

#[test]
fn test_create_task_with_swap_multi_hop_success() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, contract_id, _admin, stablecoin, oracle_id) = setup(&env);
    let oracle = MockOracleClient::new(&env, &oracle_id);

    let token_in = new_token(&env);
    let token_mid = new_token(&env);
    oracle.set_price(&token_in, &ONE);
    oracle.set_price(&stablecoin, &ONE);

    // Hop 1: token_in -> token_mid at 98%. Hop 2: token_mid -> stablecoin at 98%.
    let pool1 = new_pool(&env, 9_800);
    let pool2 = new_pool(&env, 9_800);
    StellarAssetClient::new(&env, &token_mid).mint(&pool1, &10_000);
    StellarAssetClient::new(&env, &stablecoin).mint(&pool2, &10_000);

    let creator = Address::generate(&env);
    StellarAssetClient::new(&env, &token_in).mint(&creator, &1_000);

    let mut path = Vec::new(&env);
    path.push_back(token_in.clone());
    path.push_back(token_mid.clone());
    path.push_back(stablecoin.clone());
    let mut pools = Vec::new(&env);
    pools.push_back(pool1);
    pools.push_back(pool2);
    let route = SwapRoute { path, pools };

    let tags = Vec::new(&env);
    // 1000 * 0.98 = 980; 980 * 0.98 = 960.4 -> 960 (integer division)
    let task_id = client.create_task_with_swap(
        &creator,
        &symbol_short!("title"),
        &symbol_short!("desc"),
        &token_in,
        &1_000,
        &route,
        &Some(500u32),
        &tags,
    );

    let task: Task = client.get_task(&task_id).unwrap();
    assert_eq!(task.reward, 960);

    let stablecoin_client = soroban_sdk::token::Client::new(&env, &stablecoin);
    assert_eq!(stablecoin_client.balance(&contract_id), 960);
}

#[test]
fn test_create_task_with_swap_reverts_below_slippage_minimum() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, _admin, stablecoin, oracle_id) = setup(&env);
    let oracle = MockOracleClient::new(&env, &oracle_id);

    let token_in = new_token(&env);
    oracle.set_price(&token_in, &ONE);
    oracle.set_price(&stablecoin, &ONE); // oracle says 1:1

    // Pool only pays out 50% — well below the 5% default slippage tolerance.
    let pool = new_pool(&env, 5_000);
    StellarAssetClient::new(&env, &stablecoin).mint(&pool, &10_000);

    let creator = Address::generate(&env);
    StellarAssetClient::new(&env, &token_in).mint(&creator, &1_000);

    let mut path = Vec::new(&env);
    path.push_back(token_in.clone());
    path.push_back(stablecoin.clone());
    let mut pools = Vec::new(&env);
    pools.push_back(pool);
    let route = SwapRoute { path, pools };

    let tags = Vec::new(&env);
    let result = client.try_create_task_with_swap(
        &creator,
        &symbol_short!("title"),
        &symbol_short!("desc"),
        &token_in,
        &1_000,
        &route,
        &None,
        &tags,
    );
    assert!(result.is_err(), "swap below minimum return must revert");

    // The whole invocation reverts, so no task was created and the
    // creator's input-token balance is untouched.
    let token_in_client = soroban_sdk::token::Client::new(&env, &token_in);
    assert_eq!(token_in_client.balance(&creator), 1_000);
    assert!(client.get_task(&1).is_none());
}

#[test]
fn test_create_task_with_swap_rejects_route_to_wrong_token() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _, _admin, stablecoin, oracle_id) = setup(&env);
    let oracle = MockOracleClient::new(&env, &oracle_id);

    let token_in = new_token(&env);
    let not_the_escrow_token = new_token(&env); // never the contract's TokenContract
    oracle.set_price(&token_in, &ONE);
    oracle.set_price(&not_the_escrow_token, &ONE);
    oracle.set_price(&stablecoin, &ONE);

    let pool = new_pool(&env, 10_000);
    StellarAssetClient::new(&env, &not_the_escrow_token).mint(&pool, &10_000);

    let creator = Address::generate(&env);
    StellarAssetClient::new(&env, &token_in).mint(&creator, &1_000);

    let mut path = Vec::new(&env);
    path.push_back(token_in.clone());
    path.push_back(not_the_escrow_token.clone());
    let mut pools = Vec::new(&env);
    pools.push_back(pool);
    let route = SwapRoute { path, pools };

    let tags = Vec::new(&env);
    let result = client.try_create_task_with_swap(
        &creator,
        &symbol_short!("title"),
        &symbol_short!("desc"),
        &token_in,
        &1_000,
        &route,
        &None,
        &tags,
    );
    assert!(
        result.is_err(),
        "a route ending anywhere but the escrow's configured token must be rejected"
    );

    let token_in_client = soroban_sdk::token::Client::new(&env, &token_in);
    assert_eq!(
        token_in_client.balance(&creator),
        1_000,
        "sender funds must never be pulled for a rejected route"
    );
}
