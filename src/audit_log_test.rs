#![cfg(test)]
#![allow(deprecated)]

use crate::audit_log::{self, AuditLogEntry};
use crate::{TaskManagerContract, TaskManagerContractClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::token::StellarAssetClient;
use soroban_sdk::{Address, BytesN, Env, Symbol, Vec};

fn setup_initialized_contract(env: &Env) -> (TaskManagerContractClient<'_>, Address, Address) {
    let contract_id = env.register_contract(None, TaskManagerContract);
    let client = TaskManagerContractClient::new(env, &contract_id);

    let admin = Address::generate(env);
    let token_admin = Address::generate(env);
    let token_contract = env.register_stellar_asset_contract(token_admin);
    let fee_recipient = Address::generate(env);

    client.initialize(&admin, &100u32, &token_contract, &fee_recipient);
    (client, contract_id, token_contract)
}

// ── Unit-level: module functions directly, inside a contract context ───────

#[test]
fn record_and_verify_root_matches() {
    let env = Env::default();
    let contract_id = env.register_contract(None, TaskManagerContract);
    let actor = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let root_hash: BytesN<32> = env
            .crypto()
            .sha256(&soroban_sdk::Bytes::from_array(&env, &[1u8; 4]))
            .into();

        let entry = audit_log::record_state_root(
            &env,
            Symbol::new(&env, "unit_test"),
            7,
            actor.clone(),
            root_hash.clone(),
        );

        assert_eq!(entry.index, 0);
        assert_eq!(entry.subject_id, 7);
        assert_eq!(audit_log::get_audit_log_count(&env), 1);
        assert!(audit_log::verify_audit_root(&env, 0, root_hash.clone()));

        // A different root at the same index must not verify.
        let wrong_root: BytesN<32> = env
            .crypto()
            .sha256(&soroban_sdk::Bytes::from_array(&env, &[2u8; 4]))
            .into();
        assert!(!audit_log::verify_audit_root(&env, 0, wrong_root));

        // Unknown index never verifies.
        assert!(!audit_log::verify_audit_root(&env, 99, root_hash));
    });
}

#[test]
fn get_recent_audit_log_returns_newest_first_and_respects_cap() {
    let env = Env::default();
    let contract_id = env.register_contract(None, TaskManagerContract);
    let actor = Address::generate(&env);

    env.as_contract(&contract_id, || {
        for i in 0..5u32 {
            let root_hash: BytesN<32> = env
                .crypto()
                .sha256(&soroban_sdk::Bytes::from_array(&env, &i.to_be_bytes()))
                .into();
            audit_log::record_state_root(
                &env,
                Symbol::new(&env, "seed"),
                i,
                actor.clone(),
                root_hash,
            );
        }

        let recent: Vec<AuditLogEntry> = audit_log::get_recent_audit_log(&env, 3);
        assert_eq!(recent.len(), 3);
        assert_eq!(recent.get(0).unwrap().index, 4);
        assert_eq!(recent.get(1).unwrap().index, 3);
        assert_eq!(recent.get(2).unwrap().index, 2);
    });
}

// ── Integration: real task lifecycle wiring ─────────────────────────────────

#[test]
fn complete_task_records_a_matching_state_root() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, contract_id, token_contract) = setup_initialized_contract(&env);
    let creator = Address::generate(&env);
    let assignee = Address::generate(&env);

    StellarAssetClient::new(&env, &token_contract).mint(&creator, &1000);

    let tags = Vec::new(&env);
    let task_id = client.create_task(
        &creator,
        &Symbol::new(&env, "Test_Task"),
        &Symbol::new(&env, "Desc"),
        &1000,
        &tags,
    );

    assert_eq!(client.get_audit_log_count(), 0);

    client.assign_task(&assignee, &task_id);
    assert_eq!(client.get_audit_log_count(), 1);

    client.submit_work(&assignee, &task_id, &Symbol::new(&env, "delivery_link"));
    client.complete_task(&creator, &task_id);
    assert_eq!(client.get_audit_log_count(), 2);

    // Independently recompute the root from the final on-chain task state
    // and confirm it matches what complete_task actually logged.
    let final_task = client.get_task(&task_id).unwrap();
    let recomputed_root = env.as_contract(&contract_id, || {
        audit_log::compute_state_root(&env, &final_task)
    });

    let last_entry = client.get_audit_log_entry(&1).unwrap();
    assert_eq!(last_entry.operation, Symbol::new(&env, "complete_task"));
    assert_eq!(last_entry.subject_id, task_id);
    assert_eq!(last_entry.root_hash, recomputed_root);
    assert!(client.verify_audit_root(&1, &recomputed_root));
}

#[test]
fn cancel_task_also_records_a_state_root() {
    let env = Env::default();
    env.mock_all_auths();

    let (client, _contract_id, token_contract) = setup_initialized_contract(&env);
    let creator = Address::generate(&env);
    StellarAssetClient::new(&env, &token_contract).mint(&creator, &500);

    let tags = Vec::new(&env);
    let task_id = client.create_task(
        &creator,
        &Symbol::new(&env, "Cancel_Me"),
        &Symbol::new(&env, "Desc"),
        &500,
        &tags,
    );

    client.cancel_task(&creator, &task_id);
    assert_eq!(client.get_audit_log_count(), 1);

    let entry = client.get_audit_log_entry(&0).unwrap();
    assert_eq!(entry.operation, Symbol::new(&env, "cancel_task"));
    assert_eq!(entry.subject_id, task_id);
}
