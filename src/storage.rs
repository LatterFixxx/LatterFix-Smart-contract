//! # Dual-State Ledger Storage Optimization
//!
//! This module implements a dual-state storage architecture that separates
//! high-churn data (temporary storage) from persistent data to minimize
//! state rent fees on the Soroban ledger.
//!
//! ## Storage Tiers
//!
//! | Tier | Storage Type | TTL | Use Case |
//! |------|--------------|-----|----------|
//! | **Temporary** | `temporary()` | ~7 days | Active bids, submissions, session data |
//! | **Persistent** | `persistent()` | ~31 days | User profiles, settings, reputation |
//! | **Instance** | `instance()` | Contract lifetime | Admin config, contract state |
//!
//! ## Gas Optimization Strategy
//!
//! - **High-churn data** (frequently updated): Use temporary storage to avoid
//!   repeated TTL extension costs and storage rent fees.
//! - **Low-churn data** (rarely updated): Use persistent storage for long-term
//!   durability with periodic TTL bumps.
//! - **Configuration data**: Use instance storage for contract-level settings
//!   that don't need to survive contract upgrades.

use soroban_sdk::unwrap::UnwrapOptimized;
use soroban_sdk::{contracttype, Address, Env, Symbol, Vec};

// ═══════════════════════════════════════════════════════════════════════════════
// TTL CONSTANTS
// ═══════════════════════════════════════════════════════════════════════════════

/// Maximum TTL for persistent storage (~31 days at 5s ledger close time)
pub const MAX_PERSISTENT_TTL: u32 = 5_200_000;

/// Default TTL for persistent storage (~24 days)
pub const DEFAULT_PERSISTENT_TTL: u32 = 2_073_600;

/// TTL for temporary storage (~7 days)
pub const TEMP_STORAGE_TTL: u32 = 120_960;

/// TTL for session/nonce data (~1 day)
pub const SESSION_TTL: u32 = 17_280;

/// Minimum remaining persistent-entry TTL before a renewal is triggered.
///
/// Together with `MAX_PERSISTENT_TTL`, this defines the bounded renewal
/// window for long-lived state. `extend_ttl` is a no-op while an entry is
/// above this threshold.
pub const TTL_EXTENSION_THRESHOLD: u32 = 100_000;

// ═══════════════════════════════════════════════════════════════════════════════
// STORAGE KEY ENUMS - Separated by Churn Rate
// ═══════════════════════════════════════════════════════════════════════════════

/// Keys for **persistent storage** (low-churn, long-lived data)
///
/// These are values that are written once or rarely updated:
/// - User profiles
/// - Contract statistics
/// - Categories and tags
/// - Reputation data
#[contracttype]
pub enum PersistentKey {
    /// User profile data (address -> profile)
    UserProfile(Address),
    /// Global contract statistics
    Statistics,
    /// Task categories
    Categories,
    /// Task tags index
    Tags,
    /// User reputation score
    Reputation(Address),
    /// Reputation event count per user
    ReputationEventCount(Address),
    /// Leaderboard snapshot
    Leaderboard,
}

/// Keys for **temporary storage** (high-churn, short-lived data)
///
/// These are values that change frequently during active task workflows:
/// - Active task assignments
/// - Pending submissions
/// - Active bids/proposals
/// - Session nonces
#[contracttype]
pub enum TemporaryKey {
    /// Active task being worked on (task_id -> assignment data)
    ActiveTask(u32),
    /// Pending milestone submission (task_id, milestone_id)
    PendingMilestone(u32, u32),
    /// User's active session nonce for auth
    SessionNonce(Address),
    /// Temporary bid data for tasks
    ActiveBid(u32, Address),
    /// Submission URL cache (task_id -> url)
    SubmissionCache(u32),
}

/// Keys for **instance storage** (contract configuration)
///
/// These values persist for the lifetime of the contract instance:
/// - Admin address
/// - Platform fee configuration
/// - Token contract address
/// - Initialization flag
#[contracttype]
pub enum InstanceKey {
    /// Contract admin address
    Admin,
    /// Platform fee in basis points
    PlatformFeeBps,
    /// Token contract address for payments
    TokenContract,
    /// Fee recipient address
    FeeRecipient,
    /// Initialization flag
    Initialized,
    /// Task counter
    TaskCount,
    /// Governance configuration
    GovernanceConfig,
}

// ═══════════════════════════════════════════════════════════════════════════════
// DATA STRUCTURES
// ═══════════════════════════════════════════════════════════════════════════════

/// Statistics tracking for storage optimization metrics
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StorageMetrics {
    /// Number of temporary storage reads
    pub temp_reads: u64,
    /// Number of temporary storage writes
    pub temp_writes: u64,
    /// Number of persistent storage reads
    pub persistent_reads: u64,
    /// Number of persistent storage writes
    pub persistent_writes: u64,
    /// Last TTL extension timestamp
    pub last_ttl_extension: u64,
}

/// Category for task classification
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Category {
    pub id: u32,
    pub name: Symbol,
    pub description: Symbol,
    pub task_count: u32,
    pub created_at: u64,
}

/// Global contract statistics
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct ContractStatistics {
    pub total_tasks_created: u32,
    pub total_tasks_completed: u32,
    pub total_tasks_cancelled: u32,
    pub total_tasks_disputed: u32,
    pub total_value_locked: i128,
    pub total_value_paid: i128,
    pub total_platform_fees: i128,
    pub unique_users: u32,
    pub total_milestones: u32,
}

/// Active task assignment data (temporary)
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActiveAssignment {
    pub task_id: u32,
    pub assignee: Address,
    pub assigned_at: u64,
    pub last_activity: u64,
}

/// Session nonce for authentication
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionData {
    pub nonce: u64,
    pub expires_at: u64,
}

// ═══════════════════════════════════════════════════════════════════════════════
// PERSISTENT STORAGE OPERATIONS
// ═══════════════════════════════════════════════════════════════════════════════

/// Get a value from persistent storage and renew it when it is approaching
/// archival.
///
/// Active reads are a strong signal that an entry is still live application
/// state. Renew only after a successful read so a lookup for a missing key
/// cannot trap in `extend_ttl`.
pub fn get_persistent<K, V>(env: &Env, key: &K) -> Option<V>
where
    K: soroban_sdk::IntoVal<Env, soroban_sdk::Val>,
    V: soroban_sdk::TryFromVal<Env, soroban_sdk::Val>,
{
    let value = env.storage().persistent().get(key);
    if value.is_some() {
        extend_persistent_ttl_default(env, key);
    }
    value
}

/// Set a value in persistent storage with automatic TTL
pub fn set_persistent<K, V>(env: &Env, key: &K, value: &V)
where
    K: soroban_sdk::IntoVal<Env, soroban_sdk::Val>,
    V: soroban_sdk::IntoVal<Env, soroban_sdk::Val>,
{
    env.storage().persistent().set(key, value);
    env.storage()
        .persistent()
        .extend_ttl(key, TTL_EXTENSION_THRESHOLD, DEFAULT_PERSISTENT_TTL);
}

/// Extend TTL for a persistent storage key
pub fn extend_persistent_ttl<K>(env: &Env, key: &K, threshold: u32, extend_to: u32)
where
    K: soroban_sdk::IntoVal<Env, soroban_sdk::Val>,
{
    env.storage()
        .persistent()
        .extend_ttl(key, threshold, extend_to);
}

/// Extend TTL for a persistent storage key with default values
pub fn extend_persistent_ttl_default<K>(env: &Env, key: &K)
where
    K: soroban_sdk::IntoVal<Env, soroban_sdk::Val>,
{
    env.storage()
        .persistent()
        .extend_ttl(key, TTL_EXTENSION_THRESHOLD, DEFAULT_PERSISTENT_TTL);
}

/// Remove a value from persistent storage
pub fn remove_persistent<K>(env: &Env, key: &K)
where
    K: soroban_sdk::IntoVal<Env, soroban_sdk::Val>,
{
    env.storage().persistent().remove(key);
}

/// Check if a persistent key exists
pub fn has_persistent<K>(env: &Env, key: &K) -> bool
where
    K: soroban_sdk::IntoVal<Env, soroban_sdk::Val>,
{
    env.storage().persistent().has(key)
}

// ═══════════════════════════════════════════════════════════════════════════════
// TEMPORARY STORAGE OPERATIONS (High-Churn Data)
// ═══════════════════════════════════════════════════════════════════════════════

/// Get a value from temporary storage
pub fn get_temporary<K, V>(env: &Env, key: &K) -> Option<V>
where
    K: soroban_sdk::IntoVal<Env, soroban_sdk::Val>,
    V: soroban_sdk::TryFromVal<Env, soroban_sdk::Val>,
{
    env.storage().temporary().get(key)
}

/// Set a value in temporary storage with short TTL
pub fn set_temporary<K, V>(env: &Env, key: &K, value: &V)
where
    K: soroban_sdk::IntoVal<Env, soroban_sdk::Val>,
    V: soroban_sdk::IntoVal<Env, soroban_sdk::Val>,
{
    env.storage().temporary().set(key, value);
    env.storage()
        .temporary()
        .extend_ttl(key, TTL_EXTENSION_THRESHOLD / 10, TEMP_STORAGE_TTL);
}

/// Set a value in temporary storage with custom TTL
pub fn set_temporary_with_ttl<K, V>(env: &Env, key: &K, value: &V, ttl: u32)
where
    K: soroban_sdk::IntoVal<Env, soroban_sdk::Val>,
    V: soroban_sdk::IntoVal<Env, soroban_sdk::Val>,
{
    env.storage().temporary().set(key, value);
    env.storage()
        .temporary()
        .extend_ttl(key, ttl / 2, ttl);
}

/// Extend TTL for a temporary storage key
pub fn extend_temporary_ttl<K>(env: &Env, key: &K)
where
    K: soroban_sdk::IntoVal<Env, soroban_sdk::Val>,
{
    env.storage()
        .temporary()
        .extend_ttl(key, TTL_EXTENSION_THRESHOLD / 10, TEMP_STORAGE_TTL);
}

/// Remove a value from temporary storage
pub fn remove_temporary<K>(env: &Env, key: &K)
where
    K: soroban_sdk::IntoVal<Env, soroban_sdk::Val>,
{
    env.storage().temporary().remove(key);
}

/// Check if a temporary key exists
pub fn has_temporary<K>(env: &Env, key: &K) -> bool
where
    K: soroban_sdk::IntoVal<Env, soroban_sdk::Val>,
{
    env.storage().temporary().has(key)
}

// ═══════════════════════════════════════════════════════════════════════════════
// INSTANCE STORAGE OPERATIONS (Contract Configuration)
// ═══════════════════════════════════════════════════════════════════════════════

/// Get a value from instance storage
pub fn get_instance<K, V>(env: &Env, key: &K) -> Option<V>
where
    K: soroban_sdk::IntoVal<Env, soroban_sdk::Val>,
    V: soroban_sdk::TryFromVal<Env, soroban_sdk::Val>,
{
    env.storage().instance().get(key)
}

/// Set a value in instance storage
pub fn set_instance<K, V>(env: &Env, key: &K, value: &V)
where
    K: soroban_sdk::IntoVal<Env, soroban_sdk::Val>,
    V: soroban_sdk::IntoVal<Env, soroban_sdk::Val>,
{
    env.storage().instance().set(key, value);
}

/// Remove a value from instance storage
pub fn remove_instance<K>(env: &Env, key: &K)
where
    K: soroban_sdk::IntoVal<Env, soroban_sdk::Val>,
{
    env.storage().instance().remove(key);
}

/// Check if an instance key exists
pub fn has_instance<K>(env: &Env, key: &K) -> bool
where
    K: soroban_sdk::IntoVal<Env, soroban_sdk::Val>,
{
    env.storage().instance().has(key)
}

// ═══════════════════════════════════════════════════════════════════════════════
// HIGH-LEVEL OPERATIONS - Categories
// ═══════════════════════════════════════════════════════════════════════════════

pub fn add_category(env: &Env, name: Symbol, description: Symbol) -> u32 {
    let mut categories: Vec<Category> = get_persistent(env, &PersistentKey::Categories)
        .unwrap_or_else(|| Vec::new(env));

    let id = categories.len() + 1;

    categories.push_back(Category {
        id,
        name,
        description,
        task_count: 0,
        created_at: env.ledger().timestamp(),
    });

    set_persistent(env, &PersistentKey::Categories, &categories);
    id
}

pub fn get_categories(env: &Env) -> Vec<Category> {
    get_persistent(env, &PersistentKey::Categories).unwrap_or_else(|| Vec::new(env))
}

pub fn increment_category_task_count(env: &Env, category_id: u32) {
    let mut categories: Vec<Category> = get_persistent(env, &PersistentKey::Categories)
        .unwrap_or_else(|| Vec::new(env));

    for i in 0..categories.len() {
        let mut category = categories.get(i).unwrap_optimized();
        if category.id == category_id {
            category.task_count += 1;
            categories.set(i, category);
            break;
        }
    }

    set_persistent(env, &PersistentKey::Categories, &categories);
}

// ═══════════════════════════════════════════════════════════════════════════════
// HIGH-LEVEL OPERATIONS - Statistics
// ═══════════════════════════════════════════════════════════════════════════════

pub fn get_statistics(env: &Env) -> ContractStatistics {
    get_persistent(env, &PersistentKey::Statistics).unwrap_or_default()
}

pub fn update_statistics<F>(env: &Env, updater: F)
where
    F: FnOnce(&mut ContractStatistics),
{
    let mut stats = get_statistics(env);
    updater(&mut stats);
    set_persistent(env, &PersistentKey::Statistics, &stats);
}

// ═══════════════════════════════════════════════════════════════════════════════
// HIGH-LEVEL OPERATIONS - Active Tasks (Temporary Storage)
// ═══════════════════════════════════════════════════════════════════════════════

/// Store an active task assignment (high-churn, temporary)
pub fn set_active_assignment(env: &Env, task_id: u32, assignee: Address) {
    let assignment = ActiveAssignment {
        task_id,
        assignee,
        assigned_at: env.ledger().timestamp(),
        last_activity: env.ledger().timestamp(),
    };
    set_temporary(env, &TemporaryKey::ActiveTask(task_id), &assignment);
}

/// Get an active task assignment
pub fn get_active_assignment(env: &Env, task_id: u32) -> Option<ActiveAssignment> {
    get_temporary(env, &TemporaryKey::ActiveTask(task_id))
}

/// Update last activity timestamp for an active task
pub fn touch_active_assignment(env: &Env, task_id: u32) {
    if let Some(mut assignment) = get_active_assignment(env, task_id) {
        assignment.last_activity = env.ledger().timestamp();
        set_temporary(env, &TemporaryKey::ActiveTask(task_id), &assignment);
    }
}

/// Remove an active task assignment (task completed/cancelled)
pub fn clear_active_assignment(env: &Env, task_id: u32) {
    remove_temporary(env, &TemporaryKey::ActiveTask(task_id));
}

// ═══════════════════════════════════════════════════════════════════════════════
// HIGH-LEVEL OPERATIONS - Session Nonces (Temporary Storage)
// ═══════════════════════════════════════════════════════════════════════════════

/// Create or update a session nonce
pub fn set_session_nonce(env: &Env, user: Address, nonce: u64) {
    let session = SessionData {
        nonce,
        expires_at: env.ledger().timestamp() + (SESSION_TTL as u64 * 5),
    };
    set_temporary_with_ttl(env, &TemporaryKey::SessionNonce(user), &session, SESSION_TTL);
}

/// Get session nonce for a user
pub fn get_session_nonce(env: &Env, user: Address) -> Option<SessionData> {
    get_temporary(env, &TemporaryKey::SessionNonce(user))
}

/// Clear session nonce (logout)
pub fn clear_session_nonce(env: &Env, user: Address) {
    remove_temporary(env, &TemporaryKey::SessionNonce(user));
}

// ═══════════════════════════════════════════════════════════════════════════════
// HIGH-LEVEL OPERATIONS - Submission Cache (Temporary Storage)
// ═══════════════════════════════════════════════════════════════════════════════

/// Cache a submission URL temporarily
pub fn cache_submission_url(env: &Env, task_id: u32, url: soroban_sdk::String) {
    set_temporary(env, &TemporaryKey::SubmissionCache(task_id), &url);
}

/// Get cached submission URL
pub fn get_cached_submission_url(env: &Env, task_id: u32) -> Option<soroban_sdk::String> {
    get_temporary(env, &TemporaryKey::SubmissionCache(task_id))
}

/// Clear submission cache
pub fn clear_submission_cache(env: &Env, task_id: u32) {
    remove_temporary(env, &TemporaryKey::SubmissionCache(task_id));
}

// ═══════════════════════════════════════════════════════════════════════════════
// AUTOMATIC TTL UPGRADE SCRIPTS
// ═══════════════════════════════════════════════════════════════════════════════

/// Refresh TTL for all critical persistent keys
///
/// This should be called periodically (e.g., by a bot or during high-activity periods)
/// to ensure persistent data doesn't expire.
pub fn refresh_all_persistent_ttl(env: &Env) {
    refresh_persistent_ttl_if_present(env, &PersistentKey::Statistics);
    refresh_persistent_ttl_if_present(env, &PersistentKey::Categories);
    refresh_persistent_ttl_if_present(env, &PersistentKey::Tags);
    refresh_persistent_ttl_if_present(env, &PersistentKey::Leaderboard);
}

fn refresh_persistent_ttl_if_present<K>(env: &Env, key: &K)
where
    K: soroban_sdk::IntoVal<Env, soroban_sdk::Val>,
{
    if env.storage().persistent().has(key) {
        extend_persistent_ttl_default(env, key);
    }
}

/// Migrate data from temporary to persistent storage
///
/// Used when data needs to be promoted from short-term to long-term storage,
/// e.g., when a task submission is approved and becomes historical record.
pub fn promote_to_persistent<K, V>(env: &Env, temp_key: &K, persistent_key: &K)
where
    K: soroban_sdk::IntoVal<Env, soroban_sdk::Val> + Clone,
    V: soroban_sdk::TryFromVal<Env, soroban_sdk::Val> + soroban_sdk::IntoVal<Env, soroban_sdk::Val>,
{
    let value: Option<V> = get_temporary(env, temp_key);
    if let Some(v) = value {
        set_persistent(env, persistent_key, &v);
        remove_temporary(env, temp_key);
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// BENCHMARK HELPERS
// ═══════════════════════════════════════════════════════════════════════════════

/// Get storage metrics for gas cost benchmarking
pub fn get_storage_metrics(env: &Env) -> StorageMetrics {
    // In a real implementation, these would be tracked via events or counters
    // For now, return default metrics
    StorageMetrics {
        temp_reads: 0,
        temp_writes: 0,
        persistent_reads: 0,
        persistent_writes: 0,
        last_ttl_extension: env.ledger().timestamp(),
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// LEGACY COMPATIBILITY - Deprecated Keys
// ═══════════════════════════════════════════════════════════════════════════════

/// Legacy storage key enum for backward compatibility
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    Metadata,
    TaskList,
    Categories,
    Tags,
    Statistics,
}

/// Calculate TTL based on data type (legacy helper)
pub fn calculate_ttl(_env: &Env, is_permanent: bool) -> u32 {
    if is_permanent {
        MAX_PERSISTENT_TTL
    } else {
        TEMP_STORAGE_TTL
    }
}

/// Refresh statistics TTL (legacy helper)
pub fn refresh_statistics_ttl(env: &Env) {
    extend_persistent_ttl_default(env, &PersistentKey::Statistics);
}

/// Refresh categories TTL (legacy helper)
pub fn refresh_categories_ttl(env: &Env) {
    extend_persistent_ttl_default(env, &PersistentKey::Categories);
}
