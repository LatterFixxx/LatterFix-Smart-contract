#![cfg(test)]
#![allow(deprecated)]

use crate::storage_migrator::{self, TaskV1, VersionedTask};
use crate::{TaskManagerContract, TaskStatus};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, Symbol, Vec};

fn sample_v1(env: &Env, id: u32, creator: &Address) -> TaskV1 {
    TaskV1 {
        id,
        title: Symbol::new(env, "Legacy_Task"),
        description: Symbol::new(env, "Legacy_Desc"),
        reward: 1_000,
        assignee: None,
        status: TaskStatus::Open,
        created_by: creator.clone(),
        tags: Vec::new(env),
        created_at: 100,
        updated_at: 100,
    }
}

#[test]
fn reading_a_v1_record_migrates_it_to_current_shape() {
    let env = Env::default();
    let contract_id = env.register_contract(None, TaskManagerContract);
    let creator = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let v1 = sample_v1(&env, 1, &creator);
        storage_migrator::write_versioned_task(&env, 1, &VersionedTask::V1(v1.clone()));
        assert_eq!(storage_migrator::get_migration_count(&env), 0);

        let migrated = storage_migrator::read_migrated_task(&env, 1).unwrap();
        assert_eq!(migrated.id, v1.id);
        assert_eq!(migrated.title, v1.title);
        assert_eq!(migrated.reward, v1.reward);
        assert_eq!(migrated.created_by, v1.created_by);
        assert_eq!(migrated.category_id, None); // new field, defaulted
        assert_eq!(migrated.deadline, None); // new field, defaulted
        assert_eq!(storage_migrator::get_migration_count(&env), 1);
    });
}

#[test]
fn migration_only_happens_once_per_record() {
    let env = Env::default();
    let contract_id = env.register_contract(None, TaskManagerContract);
    let creator = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let v1 = sample_v1(&env, 2, &creator);
        storage_migrator::write_versioned_task(&env, 2, &VersionedTask::V1(v1));

        let _first_read = storage_migrator::read_migrated_task(&env, 2).unwrap();
        assert_eq!(storage_migrator::get_migration_count(&env), 1);

        // Second read hits the already-migrated V2 record: no further migration.
        let _second_read = storage_migrator::read_migrated_task(&env, 2).unwrap();
        assert_eq!(storage_migrator::get_migration_count(&env), 1);
    });
}

#[test]
fn reading_a_v2_record_never_triggers_migration() {
    let env = Env::default();
    let contract_id = env.register_contract(None, TaskManagerContract);
    let creator = Address::generate(&env);

    env.as_contract(&contract_id, || {
        let task = crate::Task {
            id: 3,
            title: Symbol::new(&env, "Current_Task"),
            description: Symbol::new(&env, "Desc"),
            reward: 500,
            assignee: None,
            status: TaskStatus::Open,
            created_by: creator,
            tags: Vec::new(&env),
            category_id: Some(9),
            deadline: Some(12345),
            created_at: 200,
            updated_at: 200,
        };

        storage_migrator::write_versioned_task(&env, 3, &VersionedTask::V2(task.clone()));
        let read_back = storage_migrator::read_migrated_task(&env, 3).unwrap();

        assert_eq!(read_back.category_id, Some(9));
        assert_eq!(read_back.deadline, Some(12345));
        assert_eq!(storage_migrator::get_migration_count(&env), 0);
    });
}

#[test]
fn reading_a_missing_record_returns_none() {
    let env = Env::default();
    let contract_id = env.register_contract(None, TaskManagerContract);

    env.as_contract(&contract_id, || {
        assert!(storage_migrator::read_migrated_task(&env, 999).is_none());
    });
}

/// Benchmark showing migration overhead (issue #92's acceptance criterion).
///
/// Measures real CPU instructions consumed reading a V1 record (which
/// triggers migration + a write-back) versus reading an already-migrated
/// V2 record (the steady-state fast path), via `env.budget()`.
#[test]
fn benchmark_migration_overhead() {
    let env = Env::default();
    let contract_id = env.register_contract(None, TaskManagerContract);
    let creator = Address::generate(&env);

    let (v1_read_cost, v2_read_cost) = env.as_contract(&contract_id, || {
        storage_migrator::write_versioned_task(
            &env,
            10,
            &VersionedTask::V1(sample_v1(&env, 10, &creator)),
        );

        env.budget().reset_unlimited();
        let _ = storage_migrator::read_migrated_task(&env, 10).unwrap();
        let v1_read_cost = env.budget().cpu_instruction_cost();

        // Record at 10 is now V2 (migrated). Measure the steady-state read.
        env.budget().reset_unlimited();
        let _ = storage_migrator::read_migrated_task(&env, 10).unwrap();
        let v2_read_cost = env.budget().cpu_instruction_cost();

        (v1_read_cost, v2_read_cost)
    });

    // The migrating read does genuinely more work (decode V1 + build V2 +
    // an extra write) than the already-migrated fast path.
    assert!(v1_read_cost > v2_read_cost);
}
