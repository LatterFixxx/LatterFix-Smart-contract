#![cfg(test)]

use crate::TaskManagerContract;
use soroban_sdk::testutils::{Address as _, Events as _, Ledger as _};
use soroban_sdk::{Address, Env, IntoVal};

#[test]
fn indexed_task_event_preserves_legacy_topics_and_payload() {
    let env = Env::default();
    let id = env.register_contract(None, TaskManagerContract);
    let who = Address::generate(&env);
    env.as_contract(&id, || crate::events::emit_task_assigned(&env, 42, who.clone()));

    assert_eq!(
        env.events().all(),
        soroban_sdk::vec![
            &env,
            (
                id,
                (crate::events::topics::TASK_ASSG, 42u32).into_val(&env),
                (who, env.ledger().timestamp()).into_val(&env),
            ),
        ]
    );
}
