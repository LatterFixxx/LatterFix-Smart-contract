//! # Cross-Contract Call Reentrancy Lock using Host Invocation Key
//!
//! Provides defense-in-depth protection against cross-contract reentrancy attacks
//! and call stack exhaustion during external token transfers and untrusted callbacks.
//!
//! ## Threat Model & Defense Architecture
//! 1. **Cross-Contract Reentrancy**: During external token transfers (payouts, refunds,
//!    milestone releases), control leaves the host contract. Malicious callback
//!    contracts attempt recursive entry into task settlement functions before state
//!    transitions commit.
//! 2. **Call Stack Exhaustion**: Unbounded recursive invocations consume compute
//!    metering or host resources.
//! 3. **Defense Layers**:
//!    - **Layer 1: Host Invocation Key Inspection**: Tracks caller, target function,
//!      depth, and sequence. Intercepts any concurrent re-entry into protected functions.
//!    - **Layer 2: Strict Call Stack Depth Limit**: Rejects invocations exceeding
//!      the configured maximum call depth (default: 3).
//!    - **Layer 3: Checks-Effects-Interactions (CEI)**: State invariants mutate before
//!      external cross-contract calls dispatch.

use soroban_sdk::{contracttype, Address, Env, Symbol};
use crate::events;

// ============================================================================
// Constants
// ============================================================================

/// Default maximum call stack depth allowed for protected entry points.
pub const DEFAULT_MAX_CALL_DEPTH: u32 = 3;

// ============================================================================
// Types & Storage Keys
// ============================================================================

/// Execution status of the reentrancy mutex.
#[contracttype]
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
#[repr(u32)]
pub enum ReentrancyStatus {
    NotEntered = 0,
    Entered = 1,
}

/// Host Invocation Key capturing the execution context of a protected call.
#[contracttype]
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct HostInvocationKey {
    /// Address that initiated the protected entry call.
    pub caller: Address,
    /// Function symbol currently executing.
    pub function: Symbol,
    /// Current call stack depth at entry.
    pub depth: u32,
    /// Ledger sequence number when invocation was recorded.
    pub ledger_sequence: u32,
}

/// Instance storage keys for reentrancy control.
#[contracttype]
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum ReentrancyKey {
    /// Global execution lock status (NotEntered / Entered).
    GlobalLock,
    /// Active invocation key per function symbol.
    Invocation(Symbol),
    /// Current active call stack depth.
    CallDepth,
    /// Configured maximum allowable call depth.
    MaxCallDepth,
}

// ============================================================================
// Guard Core Functions
// ============================================================================

/// Returns the current active call stack depth.
pub fn get_call_depth(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&ReentrancyKey::CallDepth)
        .unwrap_or(0)
}

/// Returns the configured maximum allowable call depth.
pub fn get_max_call_depth(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&ReentrancyKey::MaxCallDepth)
        .unwrap_or(DEFAULT_MAX_CALL_DEPTH)
}

/// Sets the maximum allowable call depth (admin only).
pub fn set_max_call_depth(env: &Env, max_depth: u32) {
    if max_depth == 0 {
        panic!("ReentrancyGuard: max call depth must be > 0");
    }
    env.storage()
        .instance()
        .set(&ReentrancyKey::MaxCallDepth, &max_depth);
}

/// Returns whether the contract is currently entered in a protected context.
pub fn is_locked(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&ReentrancyKey::GlobalLock)
        .map(|status: ReentrancyStatus| status == ReentrancyStatus::Entered)
        .unwrap_or(false)
}

/// Inspects the active Host Invocation Key for a given function symbol, if entered.
pub fn get_invocation_key(env: &Env, function: &Symbol) -> Option<HostInvocationKey> {
    env.storage()
        .instance()
        .get(&ReentrancyKey::Invocation(function.clone()))
}

/// Enters a non-reentrant execution context.
///
/// # Invariants Enforced
/// 1. `is_locked(env) == false`: Reverts immediately on concurrent/recursive entry.
/// 2. `depth <= max_call_depth`: Reverts if call depth exceeds configured threshold.
/// 3. Updates `GlobalLock`, increments `CallDepth`, records `HostInvocationKey`, and emits event.
pub fn non_reentrant_enter(env: &Env, function: Symbol, caller: &Address) {
    // 1. Intercept reentrant call attempts
    if is_locked(env) {
        panic!("ReentrancyGuard: reentrant call intercepted");
    }

    // 2. Enforce strict call stack depth limits
    let current_depth = get_call_depth(env);
    let max_depth = get_max_call_depth(env);
    let new_depth = current_depth + 1;
    if new_depth > max_depth {
        panic!("ReentrancyGuard: max call depth exceeded");
    }

    // 3. Construct and register Host Invocation Key
    let invocation_key = HostInvocationKey {
        caller: caller.clone(),
        function: function.clone(),
        depth: new_depth,
        ledger_sequence: env.ledger().sequence(),
    };

    env.storage()
        .instance()
        .set(&ReentrancyKey::GlobalLock, &ReentrancyStatus::Entered);
    env.storage()
        .instance()
        .set(&ReentrancyKey::CallDepth, &new_depth);
    env.storage()
        .instance()
        .set(&ReentrancyKey::Invocation(function.clone()), &invocation_key);

    events::emit_reentrancy_lock_acquired(env, function, caller.clone(), new_depth);
}

/// Exits a non-reentrant execution context, restoring lock state.
///
/// # State Cleanup
/// - Decrements active call stack depth.
/// - Resets global lock to `NotEntered`.
/// - Clears function-specific `HostInvocationKey`.
/// - Emits lock release event.
pub fn non_reentrant_exit(env: &Env, function: Symbol) {
    let current_depth = get_call_depth(env);
    let new_depth = if current_depth > 0 {
        current_depth - 1
    } else {
        0
    };

    env.storage()
        .instance()
        .set(&ReentrancyKey::CallDepth, &new_depth);
    env.storage()
        .instance()
        .set(&ReentrancyKey::GlobalLock, &ReentrancyStatus::NotEntered);
    env.storage()
        .instance()
        .remove(&ReentrancyKey::Invocation(function.clone()));

    events::emit_reentrancy_lock_released(env, function, new_depth);
}
