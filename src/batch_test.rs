#![cfg(test)]
#![allow(deprecated)]

use crate::{TaskManagerContract, TaskManagerContractClient, BatchTaskLeaf, TaskStatus};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::token::StellarAssetClient;
use soroban_sdk::{Address, BytesN, Env, Symbol, Vec, xdr::ToXdr};

fn setup_env<'a>() -> (Env, TaskManagerContractClient<'a>, Address, StellarAssetClient<'a>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, TaskManagerContract);
    let client = TaskManagerContractClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token_client = StellarAssetClient::new(&env, &token_contract.address());
    
    let fee_recipient = Address::generate(&env);
    
    client.initialize(
        &admin,
        &100, // 1% fee
        &token_contract.address(),
        &fee_recipient,
    );
    
    (env, client, admin, token_client)
}

#[test]
fn test_batch_task_creation_and_claim() {
    let (env, client, _admin, token_client) = setup_env();
    let creator = Address::generate(&env);
    let assignee = Address::generate(&env);
    
    // mint tokens
    token_client.mint(&creator, &10000);
    
    let leaf = BatchTaskLeaf {
        index: 0,
        title: Symbol::new(&env, "Batch_Task_1"),
        description: Symbol::new(&env, "First_task_in_batch"),
        reward: 1000,
        assignee: Some(assignee.clone()),
    };
    
    let leaf_bytes = leaf.clone().to_xdr(&env);
    let leaf_hash: BytesN<32> = env.crypto().sha256(&leaf_bytes).into();
    
    // For 1 leaf, root = leaf_hash, proof = empty
    let root = leaf_hash.clone();
    
    let batch_id = client.create_batch_tasks(
        &creator,
        &root,
        &1000,
        &1,
    );
    
    assert_eq!(batch_id, 1);
    
    // Claim the task
    let proof = Vec::new(&env);
    let task_id = client.verify_and_claim_batch_task(
        &batch_id,
        &leaf,
        &proof,
    );
    
    assert_eq!(task_id, 1);
    
    // Verify task was created on-chain
    let task = client.get_task(&task_id).unwrap();
    assert_eq!(task.title, Symbol::new(&env, "Batch_Task_1"));
    assert_eq!(task.reward, 1000);
    assert_eq!(task.assignee, Some(assignee));
    assert_eq!(task.status, TaskStatus::Open);
}

#[test]
#[should_panic]
fn test_double_claim_fails() {
    let (env, client, _admin, token_client) = setup_env();
    let creator = Address::generate(&env);
    let assignee = Address::generate(&env);
    
    token_client.mint(&creator, &10000);
    
    let leaf = BatchTaskLeaf {
        index: 0,
        title: Symbol::new(&env, "Batch_Task_1"),
        description: Symbol::new(&env, "First_task_in_batch"),
        reward: 1000,
        assignee: Some(assignee.clone()),
    };
    
    let leaf_bytes = leaf.clone().to_xdr(&env);
    let leaf_hash: BytesN<32> = env.crypto().sha256(&leaf_bytes).into();
    let root = leaf_hash.clone();
    
    let batch_id = client.create_batch_tasks(&creator, &root, &1000, &1);
    let proof = Vec::new(&env);
    
    // First claim succeeds
    client.verify_and_claim_batch_task(&batch_id, &leaf, &proof);
    
    // Second claim fails
    client.verify_and_claim_batch_task(&batch_id, &leaf, &proof);
}
