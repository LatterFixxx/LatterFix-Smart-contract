use soroban_sdk::unwrap::UnwrapOptimized;
// Multi-asset escrow swap router.
//
// Converts arbitrary incoming SAC tokens into an approved vault stablecoin
// (e.g. USDC/ORGUSD) through one or more DEX pool hops, guarded by an
// oracle-derived minimum-return check so the conversion cannot be pushed
// through a manipulated pool price.
//
// Pool contracts are expected to expose the `PoolClient` interface — a
// generic two-asset AMM pool that receives its input token via a direct
// `transfer` (mirroring the Uniswap V2 / Soroswap pair pattern: the router
// sends tokens to the pool, then calls `swap`, which pays the output out of
// its own reserves). The oracle is expected to expose the `OracleClient`
// interface — a single `price()` entry point returning the asset price
// scaled by `ORACLE_PRICE_DECIMALS`, mirroring the Reflector oracle's
// `lastprice`.
//
// Route resolution (path validity, approved destination, oracle pricing) is
// fully checked *before* any tokens are pulled from the sender, so an
// unresolved route never touches the sender's balance. If the pools
// themselves fail to deliver the oracle-guarded minimum return, the whole
// call traps and the host transaction reverts — including the initial pull
// — which is the standard, atomic "refund" pattern used by production DEX
// routers.

use soroban_sdk::{contractclient, contracttype, Address, Env, Symbol, Vec};

use crate::DataKey;

pub const ORACLE_PRICE_DECIMALS: u32 = 7;

const BPS_DENOMINATOR: i128 = 10_000;

// ============================================================================
// External contract interfaces
// ============================================================================

#[contractclient(name = "PoolClient")]
pub trait PoolInterface {
    fn swap(
        env: Env,
        amount_in: i128,
        min_amount_out: i128,
        token_in: Address,
        token_out: Address,
        to: Address,
    ) -> i128;
}

#[contractclient(name = "OracleClient")]
pub trait OracleInterface {
    fn price(env: Env, asset: Address) -> Option<i128>;
}

// ============================================================================
// Types
// ============================================================================

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
pub struct SwapRoute {
    pub path: Vec<Address>,
    pub pools: Vec<Address>,
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
pub struct RouterConfig {
    pub oracle: Address,
    pub max_hops: u32,
    pub default_slippage_bps: u32,
}

#[contracttype]
#[derive(Clone, Eq, PartialEq, Default)]
pub struct SwapRouterStats {
    pub total_conversions: u32,
    pub total_refunds: u32,
    pub total_stablecoin_out: i128,
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
pub enum ConversionOutcome {
    Converted(Address, i128), // (token_out, amount_out)
    Refunded(Symbol),         // reason
}

#[contracttype]
pub enum SwapRouterKey {
    Config,
    ApprovedStablecoins,
    Stats,
    VaultBalance(Address, Address), // (owner, stablecoin) -> credited amount
}

// ============================================================================
// Admin configuration
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

pub fn configure(
    env: Env,
    admin: Address,
    oracle: Address,
    max_hops: u32,
    default_slippage_bps: u32,
) {
    require_admin(&env, &admin);

    if max_hops == 0 {
        panic!();
    }
    if default_slippage_bps as i128 > BPS_DENOMINATOR {
        panic!();
    }

    let config = RouterConfig {
        oracle,
        max_hops,
        default_slippage_bps,
    };
    env.storage()
        .instance()
        .set(&SwapRouterKey::Config, &config);
}

pub fn get_config(env: Env) -> RouterConfig {
    env.storage()
        .instance()
        .get(&SwapRouterKey::Config)
        .unwrap_optimized()
}

pub fn add_approved_stablecoin(env: Env, admin: Address, stablecoin: Address) {
    require_admin(&env, &admin);

    let mut list = get_approved_stablecoins(env.clone());
    if !list.contains(&stablecoin) {
        list.push_back(stablecoin);
        env.storage()
            .instance()
            .set(&SwapRouterKey::ApprovedStablecoins, &list);
    }
}

pub fn remove_approved_stablecoin(env: Env, admin: Address, stablecoin: Address) {
    require_admin(&env, &admin);

    let list = get_approved_stablecoins(env.clone());
    let mut new_list = Vec::new(&env);
    for item in list.iter() {
        if item != stablecoin {
            new_list.push_back(item);
        }
    }
    env.storage()
        .instance()
        .set(&SwapRouterKey::ApprovedStablecoins, &new_list);
}

pub fn get_approved_stablecoins(env: Env) -> Vec<Address> {
    env.storage()
        .instance()
        .get(&SwapRouterKey::ApprovedStablecoins)
        .unwrap_or_else(|| Vec::new(&env))
}

// ============================================================================
// Route validation
// ============================================================================

fn validate_route(env: &Env, config: &RouterConfig, token_in: &Address, route: &SwapRoute) -> bool {
    let hops = route.pools.len();

    if hops == 0 || route.path.len() != hops + 1 {
        return false;
    }
    if hops > config.max_hops {
        return false;
    }

    match route.path.get(0) {
        Some(first) if &first == token_in => {}
        _ => return false,
    }

    let stablecoin_out = match route.path.get(route.path.len() - 1) {
        Some(t) => t,
        None => return false,
    };

    get_approved_stablecoins(env.clone()).contains(&stablecoin_out)
}

fn validate_route_to_target(
    config: &RouterConfig,
    token_in: &Address,
    target_token: &Address,
    route: &SwapRoute,
) -> bool {
    let hops = route.pools.len();

    if hops == 0 || route.path.len() != hops + 1 {
        return false;
    }
    if hops > config.max_hops {
        return false;
    }

    match route.path.get(0) {
        Some(first) if &first == token_in => {}
        _ => return false,
    }

    match route.path.get(route.path.len() - 1) {
        Some(last) => &last == target_token,
        None => false,
    }
}

// ============================================================================
// Multi-hop swap execution wrapper
// ============================================================================

fn execute_route(env: &Env, route: &SwapRoute, amount_in: i128, vault: &Address) -> i128 {
    let hops = route.pools.len();
    let mut current_amount = amount_in;
    let this_contract = env.current_contract_address();

    for i in 0..hops {
        let token_in = route.path.get(i).unwrap_optimized();
        let token_out = route.path.get(i + 1).unwrap_optimized();
        let pool = route.pools.get(i).unwrap_optimized();
        let is_last_hop = i + 1 == hops;
        let hop_recipient = if is_last_hop {
            vault.clone()
        } else {
            this_contract.clone()
        };

        // Uniswap-V2-style pattern: send the hop's input straight to the pool,
        // then invoke it — no approve/transferFrom dance required.
        soroban_sdk::token::Client::new(env, &token_in).transfer(
            &this_contract,
            &pool,
            &current_amount,
        );

        let pool_client = PoolClient::new(env, &pool);
        current_amount =
            pool_client.swap(&current_amount, &0, &token_in, &token_out, &hop_recipient);
    }

    current_amount
}

// ============================================================================
// Vault balance bookkeeping
// ============================================================================

fn credit_vault_balance(env: &Env, owner: &Address, stablecoin: &Address, amount: i128) {
    let key = SwapRouterKey::VaultBalance(owner.clone(), stablecoin.clone());
    let current: i128 = env.storage().persistent().get(&key).unwrap_or(0);
    env.storage().persistent().set(&key, &(current + amount));
}

pub fn get_vault_balance(env: Env, owner: Address, stablecoin: Address) -> i128 {
    env.storage()
        .persistent()
        .get(&SwapRouterKey::VaultBalance(owner, stablecoin))
        .unwrap_or(0)
}

pub fn withdraw_stablecoin(env: Env, owner: Address, stablecoin: Address, amount: i128) {
    owner.require_auth();

    if amount <= 0 {
        panic!();
    }

    let key = SwapRouterKey::VaultBalance(owner.clone(), stablecoin.clone());
    let current: i128 = env.storage().persistent().get(&key).unwrap_or(0);
    if amount > current {
        panic!();
    }
    env.storage().persistent().set(&key, &(current - amount));

    soroban_sdk::token::Client::new(&env, &stablecoin).transfer(
        &env.current_contract_address(),
        &owner,
        &amount,
    );
}

// ============================================================================
// Escrow-funding entrypoint
// ============================================================================

// Swaps an arbitrary input token into `target_token` (the caller's canonical
// escrow currency) atomically, guarded by the same oracle-derived slippage
// check as `convert_incoming_deposit`. Unlike `convert_incoming_deposit`,
// the output is left in the contract's own balance — ready to fund an
// escrow lock directly — rather than credited to a per-owner vault, and the
// route's destination must match `target_token` exactly rather than any
// approved stablecoin.
//
// Auth: the caller is expected to have already authorized `sender` for this
// invocation (e.g. via `sender.require_auth()` in the task-creation entry
// point), mirroring how `create_task`'s own token transfer relies on the
// caller's up-front `require_auth`.
pub fn swap_for_funding(
    env: Env,
    sender: Address,
    token_in: Address,
    amount_in: i128,
    route: SwapRoute,
    target_token: Address,
    slippage_bps: Option<u32>,
) -> i128 {
    if amount_in <= 0 {
        panic!();
    }

    let config = get_config(env.clone());
    let slippage = slippage_bps.unwrap_or(config.default_slippage_bps);
    if slippage as i128 > BPS_DENOMINATOR {
        panic!();
    }

    if !validate_route_to_target(&config, &token_in, &target_token, &route) {
        update_stats(&env, 0, 1, 0);
        panic!();
    }

    let oracle = OracleClient::new(&env, &config.oracle);
    let price_in = match oracle.try_price(&token_in) {
        Ok(Ok(Some(p))) if p > 0 => p,
        _ => {
            update_stats(&env, 0, 1, 0);
            panic!();
        }
    };
    let price_out = match oracle.try_price(&target_token) {
        Ok(Ok(Some(p))) if p > 0 => p,
        _ => {
            update_stats(&env, 0, 1, 0);
            panic!();
        }
    };

    // Minimum acceptable output derived from oracle prices, guarding the
    // conversion against a manipulated/thin DEX pool price.
    let expected_out = amount_in
        .checked_mul(price_in)
        .unwrap_optimized()
        / price_out;
    let min_out = expected_out * (BPS_DENOMINATOR - slippage as i128) / BPS_DENOMINATOR;

    // Route and pricing are validated — now, and only now, pull the deposit.
    soroban_sdk::token::Client::new(&env, &token_in).transfer(
        &sender,
        &env.current_contract_address(),
        &amount_in,
    );

    let vault = env.current_contract_address();
    let amount_out = execute_route(&env, &route, amount_in, &vault);

    if amount_out < min_out {
        panic!();
    }

    update_stats(&env, 1, 0, amount_out);

    amount_out
}

// ============================================================================
// Statistics
// ============================================================================

pub fn get_stats(env: Env) -> SwapRouterStats {
    env.storage()
        .persistent()
        .get(&SwapRouterKey::Stats)
        .unwrap_or_default()
}

fn update_stats(env: &Env, conversions_delta: u32, refunds_delta: u32, stablecoin_out_delta: i128) {
    let mut stats = get_stats(env.clone());
    stats.total_conversions += conversions_delta;
    stats.total_refunds += refunds_delta;
    stats.total_stablecoin_out += stablecoin_out_delta;
    env.storage()
        .persistent()
        .set(&SwapRouterKey::Stats, &stats);
}

// ============================================================================
// Main entrypoint
// ============================================================================

pub fn convert_incoming_deposit(
    env: Env,
    sender: Address,
    token_in: Address,
    amount_in: i128,
    route: SwapRoute,
    slippage_bps: Option<u32>,
) -> ConversionOutcome {
    sender.require_auth();

    if amount_in <= 0 {
        panic!();
    }

    let config = get_config(env.clone());
    let slippage = slippage_bps.unwrap_or(config.default_slippage_bps);
    if slippage as i128 > BPS_DENOMINATOR {
        panic!();
    }

    if !validate_route(&env, &config, &token_in, &route) {
        update_stats(&env, 0, 1, 0);
        return ConversionOutcome::Refunded(Symbol::new(
            &env,
            "swap route could not be resolved",
        ));
    }

    let stablecoin_out = route.path.get(route.path.len() - 1).unwrap_optimized();

    let oracle = OracleClient::new(&env, &config.oracle);
    let price_in = match oracle.try_price(&token_in) {
        Ok(Ok(Some(p))) if p > 0 => p,
        _ => {
            update_stats(&env, 0, 1, 0);
            return ConversionOutcome::Refunded(Symbol::new(
                &env,
                "no oracle price for input asset",
            ));
        }
    };
    let price_out = match oracle.try_price(&stablecoin_out) {
        Ok(Ok(Some(p))) if p > 0 => p,
        _ => {
            update_stats(&env, 0, 1, 0);
            return ConversionOutcome::Refunded(Symbol::new(
                &env,
                "no oracle price for output asset",
            ));
        }
    };

    // Minimum acceptable output derived from oracle prices, guarding the
    // conversion against a manipulated/thin DEX pool price.
    let expected_out = amount_in
        .checked_mul(price_in)
        .unwrap_optimized()
        / price_out;
    let min_out = expected_out * (BPS_DENOMINATOR - slippage as i128) / BPS_DENOMINATOR;

    // Route and pricing are validated — now, and only now, pull the deposit.
    soroban_sdk::token::Client::new(&env, &token_in).transfer(
        &sender,
        &env.current_contract_address(),
        &amount_in,
    );

    let vault = env.current_contract_address();
    let amount_out = execute_route(&env, &route, amount_in, &vault);

    if amount_out < min_out {
        panic!();
    }

    credit_vault_balance(&env, &sender, &stablecoin_out, amount_out);
    update_stats(&env, 1, 0, amount_out);

    ConversionOutcome::Converted(stablecoin_out, amount_out)
}
