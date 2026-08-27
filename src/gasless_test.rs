#![cfg(test)]
#![allow(deprecated)]

//! Unit tests for signature-based gasless task assignment (`src/gasless.rs`).
//!
//! Each test builds a [`GaslessAssignment`], fetches the canonical payload the
//! contract will verify via `gasless_assignment_payload`, signs it with a real
//! `ed25519-dalek` key, and submits it through `assign_task_gasless` as a
//! relayer — proving the contributor never has to authorize anything.

use crate::gasless::GaslessAssignment;
use crate::{TaskManagerContract, TaskManagerContractClient};

use ed25519_dalek::{Signer, SigningKey};
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::token::StellarAssetClient;
use soroban_sdk::{Address, Bytes, BytesN, Env, Symbol, Vec};

// ── Setup ──────────────────────────────────────────────────────────────────

struct Fixture<'a> {
    env: Env,
    client: TaskManagerContractClient<'a>,
    contract_id: Address,
    admin: Address,
    creator: Address,
    token_contract: Address,
}

impl Fixture<'_> {
    /// Mint funding to the creator and open a fresh task; returns its id.
    fn open_task(&self, reward: i128) -> u32 {
        StellarAssetClient::new(&self.env, &self.token_contract).mint(&self.creator, &reward);
        let mut tags = Vec::new(&self.env);
        tags.push_back(Symbol::new(&self.env, "rust"));
        self.client.create_task(
            &self.creator,
            &Symbol::new(&self.env, "Task"),
            &Symbol::new(&self.env, "Desc"),
            &reward,
            &tags,
        )
    }
}

fn setup() -> (Fixture<'static>, u32) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, TaskManagerContract);
    let client = TaskManagerContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract(token_admin);
    let fee_recipient = Address::generate(&env);
    client.initialize(&admin, &100, &token_contract, &fee_recipient);

    let creator = Address::generate(&env);

    let fx = Fixture {
        env,
        client,
        contract_id,
        admin,
        creator,
        token_contract,
    };
    let task_id = fx.open_task(1_000);
    (fx, task_id)
}

fn signing_key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

fn pubkey(env: &Env, sk: &SigningKey) -> BytesN<32> {
    BytesN::from_array(env, &sk.verifying_key().to_bytes())
}

/// Sign the contract-canonical payload for a request with `sk`.
fn sign(env: &Env, payload: &Bytes, sk: &SigningKey) -> BytesN<64> {
    let buffer = payload.to_buffer::<512>();
    let bytes = sk.sign(buffer.as_slice()).to_bytes();
    BytesN::from_array(env, &bytes)
}

fn request(
    fx: &Fixture,
    contributor: &Address,
    task_id: u32,
    nonce: u64,
    expiration_ledger: u32,
) -> GaslessAssignment {
    GaslessAssignment {
        contract: fx.contract_id.clone(),
        task_id,
        contributor: contributor.clone(),
        nonce,
        expiration_ledger,
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[test]
fn test_gasless_assignment_happy_path() {
    let (fx, task_id) = setup();
    let contributor = Address::generate(&fx.env);
    let relayer = Address::generate(&fx.env);
    let sk = signing_key(1);

    fx.client
        .register_signing_key(&contributor, &pubkey(&fx.env, &sk));

    let req = request(&fx, &contributor, task_id, 0, 1_000);
    let payload = fx.client.gasless_assignment_payload(&req);
    let sig = sign(&fx.env, &payload, &sk);

    assert_eq!(fx.client.get_assignment_nonce(&contributor), 0);
    fx.client.assign_task_gasless(&relayer, &req, &sig);

    // Gasless: the relayer authorized the call, the contributor did not.
    // (Captured immediately — later view calls would reset `auths()`.)
    let auths = fx.env.auths();
    assert!(auths.iter().any(|(a, _)| *a == relayer));
    assert!(!auths.iter().any(|(a, _)| *a == contributor));

    let task = fx.client.get_task(&task_id).unwrap();
    assert_eq!(task.assignee, Some(contributor.clone()));
    assert_eq!(task.status, crate::TaskStatus::InProgress);
    assert_eq!(fx.client.get_assignment_nonce(&contributor), 1);
}

#[test]
#[should_panic]
fn test_replay_rejected() {
    let (fx, task_id) = setup();
    let contributor = Address::generate(&fx.env);
    let relayer = Address::generate(&fx.env);
    let sk = signing_key(2);
    fx.client
        .register_signing_key(&contributor, &pubkey(&fx.env, &sk));

    let req = request(&fx, &contributor, task_id, 0, 1_000);
    let sig = sign(&fx.env, &fx.client.gasless_assignment_payload(&req), &sk);

    fx.client.assign_task_gasless(&relayer, &req, &sig);
    // Same signed message again — nonce already consumed.
    fx.client.assign_task_gasless(&relayer, &req, &sig);
}

#[test]
#[should_panic]
fn test_wrong_signer_rejected() {
    let (fx, task_id) = setup();
    let contributor = Address::generate(&fx.env);
    let relayer = Address::generate(&fx.env);
    fx.client
        .register_signing_key(&contributor, &pubkey(&fx.env, &signing_key(3)));

    let req = request(&fx, &contributor, task_id, 0, 1_000);
    // Signed by a different key than the one registered.
    let sig = sign(
        &fx.env,
        &fx.client.gasless_assignment_payload(&req),
        &signing_key(99),
    );
    fx.client.assign_task_gasless(&relayer, &req, &sig);
}

#[test]
#[should_panic]
fn test_tampered_field_rejected() {
    let (fx, task_id) = setup();
    let contributor = Address::generate(&fx.env);
    let attacker = Address::generate(&fx.env);
    let relayer = Address::generate(&fx.env);
    let sk = signing_key(4);
    fx.client
        .register_signing_key(&contributor, &pubkey(&fx.env, &sk));

    let signed = request(&fx, &contributor, task_id, 0, 1_000);
    let sig = sign(&fx.env, &fx.client.gasless_assignment_payload(&signed), &sk);

    // Relayer swaps the beneficiary after the contributor signed.
    let tampered = request(&fx, &attacker, task_id, 0, 1_000);
    fx.client.assign_task_gasless(&relayer, &tampered, &sig);
}

#[test]
#[should_panic]
fn test_expired_request_rejected() {
    let (fx, task_id) = setup();
    let contributor = Address::generate(&fx.env);
    let relayer = Address::generate(&fx.env);
    let sk = signing_key(5);
    fx.client
        .register_signing_key(&contributor, &pubkey(&fx.env, &sk));

    fx.env.ledger().set_sequence_number(50);
    let req = request(&fx, &contributor, task_id, 0, 10); // already past
    let sig = sign(&fx.env, &fx.client.gasless_assignment_payload(&req), &sk);
    fx.client.assign_task_gasless(&relayer, &req, &sig);
}

#[test]
#[should_panic]
fn test_unregistered_contributor_rejected() {
    let (fx, task_id) = setup();
    let contributor = Address::generate(&fx.env);
    let relayer = Address::generate(&fx.env);
    let sk = signing_key(6);

    let req = request(&fx, &contributor, task_id, 0, 1_000);
    let sig = sign(&fx.env, &fx.client.gasless_assignment_payload(&req), &sk);
    // No register_signing_key call.
    fx.client.assign_task_gasless(&relayer, &req, &sig);
}

#[test]
#[should_panic]
fn test_wrong_contract_rejected() {
    let (fx, task_id) = setup();
    let contributor = Address::generate(&fx.env);
    let relayer = Address::generate(&fx.env);
    let sk = signing_key(7);
    fx.client
        .register_signing_key(&contributor, &pubkey(&fx.env, &sk));

    let mut req = request(&fx, &contributor, task_id, 0, 1_000);
    req.contract = Address::generate(&fx.env); // bound to a different deployment
    let sig = sign(&fx.env, &fx.client.gasless_assignment_payload(&req), &sk);
    fx.client.assign_task_gasless(&relayer, &req, &sig);
}

#[test]
#[should_panic]
fn test_non_open_task_rejected() {
    let (fx, task_id) = setup();
    let first = Address::generate(&fx.env);
    let second = Address::generate(&fx.env);
    let relayer = Address::generate(&fx.env);
    let sk1 = signing_key(8);
    let sk2 = signing_key(9);
    fx.client.register_signing_key(&first, &pubkey(&fx.env, &sk1));
    fx.client.register_signing_key(&second, &pubkey(&fx.env, &sk2));

    let req1 = request(&fx, &first, task_id, 0, 1_000);
    let sig1 = sign(&fx.env, &fx.client.gasless_assignment_payload(&req1), &sk1);
    fx.client.assign_task_gasless(&relayer, &req1, &sig1);

    // Task is now InProgress — a second gasless claim must fail.
    let req2 = request(&fx, &second, task_id, 0, 1_000);
    let sig2 = sign(&fx.env, &fx.client.gasless_assignment_payload(&req2), &sk2);
    fx.client.assign_task_gasless(&relayer, &req2, &sig2);
}

#[test]
fn test_admin_set_signing_key_enables_full_gasless_onboarding() {
    let (fx, task_id) = setup();
    let contributor = Address::generate(&fx.env);
    let relayer = Address::generate(&fx.env);
    let sk = signing_key(10);

    // Contributor never touches the chain; admin registers their key.
    fx.client
        .admin_set_signing_key(&fx.admin, &contributor, &pubkey(&fx.env, &sk));
    assert_eq!(
        fx.client.get_signing_key(&contributor),
        Some(pubkey(&fx.env, &sk))
    );

    let req = request(&fx, &contributor, task_id, 0, 1_000);
    let sig = sign(&fx.env, &fx.client.gasless_assignment_payload(&req), &sk);
    fx.client.assign_task_gasless(&relayer, &req, &sig);

    assert_eq!(
        fx.client.get_task(&task_id).unwrap().assignee,
        Some(contributor)
    );
}

#[test]
#[should_panic]
fn test_admin_set_signing_key_rejects_non_admin() {
    let (fx, _task_id) = setup();
    let not_admin = Address::generate(&fx.env);
    let contributor = Address::generate(&fx.env);
    fx.client.admin_set_signing_key(
        &not_admin,
        &contributor,
        &pubkey(&fx.env, &signing_key(11)),
    );
}

#[test]
fn test_nonce_advances_across_sequential_assignments() {
    let (fx, first_task) = setup();
    let contributor = Address::generate(&fx.env);
    let relayer = Address::generate(&fx.env);
    let sk = signing_key(12);
    fx.client
        .register_signing_key(&contributor, &pubkey(&fx.env, &sk));

    let req0 = request(&fx, &contributor, first_task, 0, 1_000);
    let sig0 = sign(&fx.env, &fx.client.gasless_assignment_payload(&req0), &sk);
    fx.client.assign_task_gasless(&relayer, &req0, &sig0);
    assert_eq!(fx.client.get_assignment_nonce(&contributor), 1);

    let second_task = fx.open_task(500);
    let req1 = request(&fx, &contributor, second_task, 1, 1_000);
    let sig1 = sign(&fx.env, &fx.client.gasless_assignment_payload(&req1), &sk);
    fx.client.assign_task_gasless(&relayer, &req1, &sig1);
    assert_eq!(fx.client.get_assignment_nonce(&contributor), 2);

    // A stale nonce (0) is now rejected.
    let stale = request(&fx, &contributor, second_task, 0, 1_000);
    let stale_sig = sign(&fx.env, &fx.client.gasless_assignment_payload(&stale), &sk);
    assert!(fx
        .client
        .try_assign_task_gasless(&relayer, &stale, &stale_sig)
        .is_err());
}
