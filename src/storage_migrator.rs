//! Versioned storage struct + auto-migration framework (issue #92).
//!
//! Worked example on `Task`: `TaskV1` models a pre-existing on-chain shape
//! (before `category_id`/`deadline` existed), `VersionedTask` is the
//! version-tagged envelope actually written to storage — `#[contracttype]`
//! enums already encode their variant as a discriminant, so the variant
//! itself *is* the version tag — and `read_migrated_task` transparently
//! upgrades a `V1` record to the current `Task` shape the moment it's read,
//! writing the migrated record back so the upgrade only ever costs once per
//! record.
//!
//! This lives under its own storage key namespace (`MigratorKey`) rather
//! than the contract's live `DataKey::Task` path: there is no deployed V1
//! data for that path to migrate from yet, and retrofitting every one of
//! `DataKey::Task`'s ~20 read/write call sites in `lib.rs` is a separate,
//! larger change than this issue's scope. What follows is the mechanism a
//! future `Task` shape change would plug into at those sites, demonstrated
//! and benchmarked end-to-end.

use soroban_sdk::{contracttype, Address, Env, Symbol, Vec};

use crate::{Task, TaskStatus};

/// `Task`'s shape before `category_id` and `deadline` existed.
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, kani), derive(Debug))]
pub struct TaskV1 {
    pub id: u32,
    pub title: Symbol,
    pub description: Symbol,
    pub reward: i128,
    pub assignee: Option<Address>,
    pub status: TaskStatus,
    pub created_by: Address,
    pub tags: Vec<Symbol>,
    pub created_at: u64,
    pub updated_at: u64,
}

/// Version-tagged envelope. Add a `V3(TaskV3)` variant the next time
/// `Task`'s shape changes, and a `migrate_task_v2_to_v3` alongside it.
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, kani), derive(Debug))]
pub enum VersionedTask {
    V1(TaskV1),
    V2(Task),
}

#[contracttype]
pub enum MigratorKey {
    VersionedTask(u32),
    MigrationCount,
}

/// Migrate a V1 record to the current (V2) `Task` shape. Fields that didn't
/// exist in V1 default to `None` rather than a guessed value, since V1
/// records predate them by definition.
pub fn migrate_task_v1_to_v2(v1: TaskV1) -> Task {
    Task {
        id: v1.id,
        title: v1.title,
        description: v1.description,
        reward: v1.reward,
        assignee: v1.assignee,
        status: v1.status,
        created_by: v1.created_by,
        tags: v1.tags,
        category_id: None,
        deadline: None,
        created_at: v1.created_at,
        updated_at: v1.updated_at,
    }
}

/// Write a versioned task record.
pub fn write_versioned_task(env: &Env, task_id: u32, versioned: &VersionedTask) {
    env.storage()
        .persistent()
        .set(&MigratorKey::VersionedTask(task_id), versioned);
    crate::storage::extend_persistent_ttl(
        env,
        &MigratorKey::VersionedTask(task_id),
        100_000,
        crate::storage::DEFAULT_PERSISTENT_TTL,
    );
}

/// Read a task record, transparently migrating a `V1` record to the current
/// shape and persisting the migrated record so later reads take the fast,
/// already-current-version path. Returns `None` if nothing is stored.
pub fn read_migrated_task(env: &Env, task_id: u32) -> Option<Task> {
    let versioned: VersionedTask = env
        .storage()
        .persistent()
        .get(&MigratorKey::VersionedTask(task_id))?;

    match versioned {
        VersionedTask::V2(task) => Some(task),
        VersionedTask::V1(v1) => {
            let migrated = migrate_task_v1_to_v2(v1);
            write_versioned_task(env, task_id, &VersionedTask::V2(migrated.clone()));

            let count: u32 = env
                .storage()
                .instance()
                .get(&MigratorKey::MigrationCount)
                .unwrap_or(0);
            env.storage()
                .instance()
                .set(&MigratorKey::MigrationCount, &(count + 1));

            Some(migrated)
        }
    }
}

/// Total number of V1 -> V2 migrations performed so far.
pub fn get_migration_count(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&MigratorKey::MigrationCount)
        .unwrap_or(0)
}
