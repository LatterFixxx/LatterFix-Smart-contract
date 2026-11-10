/* Authorized Protocol Quality Assurance & Formal Invariant Verification */
#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Events},
    Address, BytesN, Env, Symbol,
};
use crate::events::{
    emit_correlated_operation, emit_state_transition_snapshot, DiagnosticEventMetadata,
};
use crate::TaskManagerContract;

#[test]
fn test_state_transition_snapshot_event_publishing() {
    let env = Env::default();
    let contract_id = env.register_contract(None, TaskManagerContract);
    let correlation_id = BytesN::from_array(&env, &[0x42u8; 32]);
    let entity_id = 101u32;
    let prev_state = Symbol::new(&env, "Open");
    let next_state = Symbol::new(&env, "Assigned");

    env.as_contract(&contract_id, || {
        emit_state_transition_snapshot(&env, correlation_id.clone(), entity_id, prev_state.clone(), next_state.clone());
    });

    let all_events = env.events().all();
    assert!(!all_events.is_empty(), "Diagnostic snapshot event should be published");
    let last_event = all_events.last().unwrap();
    assert_eq!(last_event.1.len(), 2, "Topic vector should contain [diag_snp, entity_id]");
}

#[test]
fn test_correlated_operation_event_publishing() {
    let env = Env::default();
    let contract_id = env.register_contract(None, TaskManagerContract);
    let correlation_id = BytesN::from_array(&env, &[0x77u8; 32]);
    let entity_id = 202u32;
    let op_tag = Symbol::new(&env, "submit_work");
    let actor = Address::generate(&env);

    env.as_contract(&contract_id, || {
        emit_correlated_operation(&env, correlation_id.clone(), entity_id, op_tag.clone(), actor.clone());
    });

    let all_events = env.events().all();
    assert!(!all_events.is_empty(), "Correlated operation event should be published");
}

#[test]
fn test_diagnostic_metadata_struct() {
    let env = Env::default();
    let meta = DiagnosticEventMetadata {
        correlation_id: BytesN::from_array(&env, &[0x01u8; 32]),
        sequence: 12345,
        timestamp: 1700000000,
    };
    assert_eq!(meta.sequence, 12345);
    assert_eq!(meta.timestamp, 1700000000);
}
