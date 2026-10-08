#![cfg(test)]

use crate::{
    storage::{
        self, ContractStatistics, PersistentKey, DEFAULT_PERSISTENT_TTL,
        TTL_EXTENSION_THRESHOLD,
    },
    TaskManagerContract, TaskManagerContractClient,
};
use soroban_sdk::{
    testutils::{storage::Persistent as _, Address as _},
    Address, Env,
};

#[test]
fn persistent_read_renews_entry_near_archival() {
    let env = Env::default();
    let contract_id = env.register_contract(None, TaskManagerContract);

    env.as_contract(&contract_id, || {
        // Write directly so the entry starts at the test network's minimum
        // persistent TTL rather than going through set_persistent's write bump.
        env.storage()
            .persistent()
            .set(&PersistentKey::Statistics, &ContractStatistics::default());

        let before = env
            .storage()
            .persistent()
            .get_ttl(&PersistentKey::Statistics);
        assert!(before < TTL_EXTENSION_THRESHOLD);

        let value: Option<ContractStatistics> =
            storage::get_persistent(&env, &PersistentKey::Statistics);
        assert!(value.is_some());

        let after = env
            .storage()
            .persistent()
            .get_ttl(&PersistentKey::Statistics);
        assert_eq!(after, DEFAULT_PERSISTENT_TTL);
    });
}

#[test]
fn maintenance_refresh_is_admin_only_and_skips_missing_keys() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, TaskManagerContract);
    let client = TaskManagerContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract(token_admin);
    let fee_recipient = Address::generate(&env);

    client.initialize(&admin, &100, &token_contract, &fee_recipient);

    env.as_contract(&contract_id, || {
        // Only Statistics exists. The maintenance sweep must safely skip
        // Categories/Tags/Leaderboard until they are created.
        env.storage()
            .persistent()
            .set(&PersistentKey::Statistics, &ContractStatistics::default());
    });

    let attacker = Address::generate(&env);
    assert!(client.try_refresh_persistent_storage(&attacker).is_err());

    client.refresh_persistent_storage(&admin);

    env.as_contract(&contract_id, || {
        assert_eq!(
            env.storage()
                .persistent()
                .get_ttl(&PersistentKey::Statistics),
            DEFAULT_PERSISTENT_TTL
        );
    });
}
