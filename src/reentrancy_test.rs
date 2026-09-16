use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, Symbol, Vec,
};

// ============================================================================
// Malicious Callback Token Mock for Reentrancy Simulation
// ============================================================================

#[contract]
pub struct MaliciousCallbackToken;

#[contractimpl]
impl MaliciousCallbackToken {
    pub fn setup(env: Env, target_contract: Address, target_task_id: u32, caller: Address, action: Symbol) {
        env.storage().instance().set(&Symbol::new(&env, "target"), &target_contract);
        env.storage().instance().set(&Symbol::new(&env, "task_id"), &target_task_id);
        env.storage().instance().set(&Symbol::new(&env, "caller"), &caller);
        env.storage().instance().set(&Symbol::new(&env, "action"), &action);
    }

    pub fn transfer(env: Env, _from: Address, _to: Address, _amount: i128) {
        let action_opt: Option<Symbol> = env.storage().instance().get(&Symbol::new(&env, "action"));
        if let Some(action) = action_opt {
            let target: Address = env.storage().instance().get(&Symbol::new(&env, "target")).unwrap();
            let task_id: u32 = env.storage().instance().get(&Symbol::new(&env, "task_id")).unwrap();
            let caller: Address = env.storage().instance().get(&Symbol::new(&env, "caller")).unwrap();

            let client = TaskManagerContractClient::new(&env, &target);
            if action == Symbol::new(&env, "complete") {
                // Attempt recursive reentrancy into payout flow
                client.complete_task(&caller, &task_id);
            } else if action == Symbol::new(&env, "cancel") {
                // Attempt recursive reentrancy into cancellation refund flow
                client.cancel_task(&caller, &task_id);
            }
        }
    }

    pub fn balance(_env: Env, _id: Address) -> i128 {
        1_000_000
    }
}

// ============================================================================
// Test Setup Helpers
// ============================================================================

fn setup_test_env<'a>() -> (
    Env,
    TaskManagerContractClient<'a>,
    Address,
    Address,
    Address,
    Address,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, TaskManagerContract);
    let client = TaskManagerContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let assignee = Address::generate(&env);
    let fee_recipient = Address::generate(&env);

    // Standard mock token contract
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin);
    let _token_client = soroban_sdk::token::Client::new(&env, &token_contract.address());
    let token_admin_client =
        soroban_sdk::token::StellarAssetClient::new(&env, &token_contract.address());

    token_admin_client.mint(&creator, &100_000);
    token_admin_client.mint(&contract_id, &100_000);

    client.initialize(
        &admin,
        &250, // 2.5% fee
        &token_contract.address(),
        &fee_recipient,
    );

    (
        env,
        client,
        contract_id,
        admin,
        creator,
        assignee,
        fee_recipient,
    )
}

// ============================================================================
// Security Tests: Reentrancy Interception & Host Invocation Key Guard
// ============================================================================

#[test]
fn test_reentrancy_initial_state_and_getters() {
    let (env, client, _contract_id, admin, _creator, _assignee, _fee_recipient) =
        setup_test_env();

    // Verify initial reentrancy state
    assert_eq!(client.get_reentrancy_status(), false);
    assert_eq!(client.get_call_depth(), 0);
    assert_eq!(client.get_max_call_depth(), reentrancy_guard::DEFAULT_MAX_CALL_DEPTH);

    let fn_sym = Symbol::new(&env, "complete_task");
    assert!(client.get_invocation_key(&fn_sym).is_none());

    // Admin updates max call depth
    client.set_max_call_depth(&admin, &5);
    assert_eq!(client.get_max_call_depth(), 5);
}

#[test]
#[should_panic(expected = "Unauthorized: caller is not admin")]
fn test_set_max_call_depth_unauthorized() {
    let (_env, client, _contract_id, _admin, non_admin, _assignee, _fee_recipient) =
        setup_test_env();

    client.set_max_call_depth(&non_admin, &10);
}

#[test]
#[should_panic(expected = "ReentrancyGuard: max call depth must be > 0")]
fn test_set_max_call_depth_zero_invalid() {
    let (_env, client, _contract_id, admin, _non_admin, _assignee, _fee_recipient) =
        setup_test_env();

    client.set_max_call_depth(&admin, &0);
}

#[test]
fn test_normal_task_flow_leaves_reentrancy_lock_clean() {
    let (env, client, _contract_id, _admin, creator, assignee, _fee_recipient) =
        setup_test_env();

    let tags = Vec::new(&env);
    let task_id = client.create_task(
        &creator,
        &Symbol::new(&env, "Security_Task"),
        &Symbol::new(&env, "Inspect_Guard"),
        &1000,
        &tags,
    );

    client.assign_task(&assignee, &task_id);
    client.submit_work(
        &assignee,
        &task_id,
        &Symbol::new(&env, "proof_delivered"),
    );

    // Prior to complete_task: lock is inactive, depth is 0
    assert_eq!(client.get_reentrancy_status(), false);
    assert_eq!(client.get_call_depth(), 0);

    client.complete_task(&creator, &task_id);

    // After complete_task: lock is cleanly released, depth is 0, invocation key is removed
    assert_eq!(client.get_reentrancy_status(), false);
    assert_eq!(client.get_call_depth(), 0);
    assert!(client.get_invocation_key(&Symbol::new(&env, "complete_task")).is_none());

    let task = client.get_task(&task_id).unwrap();
    assert_eq!(task.status, TaskStatus::Verified);
}

#[test]
fn test_cancel_task_leaves_reentrancy_lock_clean() {
    let (env, client, _contract_id, _admin, creator, _assignee, _fee_recipient) =
        setup_test_env();

    let tags = Vec::new(&env);
    let task_id = client.create_task(
        &creator,
        &Symbol::new(&env, "Cancel_Task"),
        &Symbol::new(&env, "To_Be_Cancelled"),
        &1000,
        &tags,
    );

    client.cancel_task(&creator, &task_id);

    // After cancel_task: lock is cleanly released, depth is 0
    assert_eq!(client.get_reentrancy_status(), false);
    assert_eq!(client.get_call_depth(), 0);
    assert!(client.get_invocation_key(&Symbol::new(&env, "cancel_task")).is_none());

    let task = client.get_task(&task_id).unwrap();
    assert_eq!(task.status, TaskStatus::Cancelled);
}

#[test]
#[should_panic(expected = "ReentrancyGuard: reentrant call intercepted")]
fn test_direct_reentrancy_guard_panic() {
    let env = Env::default();
    let contract_id = env.register_contract(None, TaskManagerContract);
    let caller = Address::generate(&env);
    let f = Symbol::new(&env, "test_fn");

    env.as_contract(&contract_id, || {
        reentrancy_guard::non_reentrant_enter(&env, f.clone(), &caller);
        // Attempt reentrant entry
        reentrancy_guard::non_reentrant_enter(&env, f, &caller);
    });
}

#[test]
#[should_panic]
fn test_intercept_cross_contract_reentrancy_on_complete_task() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, TaskManagerContract);
    let client = TaskManagerContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let assignee = Address::generate(&env);
    let fee_recipient = Address::generate(&env);

    // Register malicious token contract that recalls complete_task during transfer
    let mal_token_id = env.register_contract(None, MaliciousCallbackToken);
    let mal_token_client = MaliciousCallbackTokenClient::new(&env, &mal_token_id);

    client.initialize(
        &admin,
        &0, // 0 fee for direct test
        &mal_token_id,
        &fee_recipient,
    );

    let tags = Vec::new(&env);
    let task_id = client.create_task(
        &creator,
        &Symbol::new(&env, "Reentrancy_Test"),
        &Symbol::new(&env, "Desc"),
        &500,
        &tags,
    );
    client.assign_task(&assignee, &task_id);
    client.submit_work(
        &assignee,
        &task_id,
        &Symbol::new(&env, "deliver"),
    );

    // Setup malicious callback contract targeting complete_task
    mal_token_client.setup(
        &contract_id,
        &task_id,
        &creator,
        &Symbol::new(&env, "complete"),
    );

    // When complete_task is called, token_client.transfer dispatches to MaliciousCallbackToken.
    // MaliciousCallbackToken attempts to recall client.complete_task.
    // ReentrancyGuard MUST intercept this and panic!
    client.complete_task(&creator, &task_id);
}

#[test]
#[should_panic]
fn test_intercept_cross_contract_reentrancy_on_cancel_task() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, TaskManagerContract);
    let client = TaskManagerContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let creator = Address::generate(&env);
    let fee_recipient = Address::generate(&env);

    // Register malicious token contract targeting cancel_task
    let mal_token_id = env.register_contract(None, MaliciousCallbackToken);
    let mal_token_client = MaliciousCallbackTokenClient::new(&env, &mal_token_id);

    client.initialize(
        &admin,
        &0,
        &mal_token_id,
        &fee_recipient,
    );

    let tags = Vec::new(&env);
    let task_id = client.create_task(
        &creator,
        &Symbol::new(&env, "Cancel_Reentrant"),
        &Symbol::new(&env, "Desc"),
        &500,
        &tags,
    );

    mal_token_client.setup(
        &contract_id,
        &task_id,
        &creator,
        &Symbol::new(&env, "cancel"),
    );

    // Should be intercepted by ReentrancyGuard on recursive cancel_task call
    client.cancel_task(&creator, &task_id);
}

#[test]
fn test_host_invocation_key_recording_and_depth() {
    let env = Env::default();
    env.ledger().set_sequence_number(42);

    let contract_id = env.register_contract(None, TaskManagerContract);
    let caller = Address::generate(&env);
    let fn_sym = Symbol::new(&env, "payout_entry");

    env.as_contract(&contract_id, || {
        // Initially not locked, depth 0
        assert_eq!(reentrancy_guard::is_locked(&env), false);
        assert_eq!(reentrancy_guard::get_call_depth(&env), 0);

        // Enter context
        reentrancy_guard::non_reentrant_enter(&env, fn_sym.clone(), &caller);

        // Invariants while inside
        assert_eq!(reentrancy_guard::is_locked(&env), true);
        assert_eq!(reentrancy_guard::get_call_depth(&env), 1);

        let key = reentrancy_guard::get_invocation_key(&env, &fn_sym).unwrap();
        assert_eq!(key.caller, caller);
        assert_eq!(key.function, fn_sym);
        assert_eq!(key.depth, 1);
        assert_eq!(key.ledger_sequence, 42);

        // Exit context
        reentrancy_guard::non_reentrant_exit(&env, fn_sym.clone());

        // Invariants after exit
        assert_eq!(reentrancy_guard::is_locked(&env), false);
        assert_eq!(reentrancy_guard::get_call_depth(&env), 0);
        assert!(reentrancy_guard::get_invocation_key(&env, &fn_sym).is_none());
    });
}

#[test]
#[should_panic(expected = "ReentrancyGuard: max call depth exceeded")]
fn test_call_stack_depth_limit_exceeded() {
    let env = Env::default();
    let contract_id = env.register_contract(None, TaskManagerContract);
    let caller = Address::generate(&env);

    env.as_contract(&contract_id, || {
        reentrancy_guard::set_max_call_depth(&env, 2);

        // Simulate nested calls exceeding depth limit
        let f1 = Symbol::new(&env, "step1");
        let f2 = Symbol::new(&env, "step2");
        let f3 = Symbol::new(&env, "step3");

        // Manually manipulate call depth to simulate nested depth checks
        reentrancy_guard::non_reentrant_enter(&env, f1, &caller);
        // Release lock temporarily to test depth check on second enter
        env.storage().instance().set(&reentrancy_guard::ReentrancyKey::GlobalLock, &reentrancy_guard::ReentrancyStatus::NotEntered);
        reentrancy_guard::non_reentrant_enter(&env, f2, &caller);
        env.storage().instance().set(&reentrancy_guard::ReentrancyKey::GlobalLock, &reentrancy_guard::ReentrancyStatus::NotEntered);

        // 3rd call exceeds max depth of 2!
        reentrancy_guard::non_reentrant_enter(&env, f3, &caller);
    });
}
