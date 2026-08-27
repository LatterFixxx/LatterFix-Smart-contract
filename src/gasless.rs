// Cryptographic signature-based *gasless* task assignment.
//
// Lets a contributor claim an open task without holding XLM or submitting a
// transaction themselves. The contributor signs a structured authorization
// message **off-chain** with their Ed25519 key; a **gas relayer** submits it
// on-chain through [`assign_task_gasless`] and pays the fees. The contract
// re-derives the exact signed bytes from the call arguments, verifies the
// signature against the contributor's registered key, and enforces a
// per-contributor **nonce** so a captured message can never be replayed.
//
// ### Off-chain flow
//
// 1. The contributor registers an Ed25519 public key once, either themselves
//    via [`register_signing_key`] or through an admin via
//    [`admin_set_signing_key`] (so onboarding can itself be gasless).
// 2. The contributor's client builds a [`GaslessAssignment`]:
//
//    ```json
//    {
//      "contract":          "C...",   // this contract's address (domain separation)
//      "task_id":           42,
//      "contributor":       "G...",   // the address that will be assigned
//      "nonce":             0,        // == current on-chain nonce for `contributor`
//      "expiration_ledger": 1_234_567 // request is void once the ledger passes this
//    }
//    ```
//
// 3. The client fetches the canonical bytes to sign from
//    [`assignment_payload`] (the SDK XDR encoding of the struct) and signs
//    them with the contributor's Ed25519 secret key, producing a 64-byte
//    signature.
// 4. The relayer calls [`assign_task_gasless`] with `(relayer, request,
//    signature)`. Only `relayer` authorizes the transaction.
//
// ### On-chain validation (all failures panic, reverting the transaction)
//
// * `request.contract` must equal this contract's address — a signature made
//   for one deployment can't be replayed against another.
// * the current ledger sequence must not have passed `expiration_ledger`.
// * `request.nonce` must equal the contributor's current nonce; it is
//   incremented on success, so the same signed message is single-use.
// * the contributor must have a registered signing key.
// * `env.crypto().ed25519_verify` must accept the signature over the
//   re-encoded request bytes.
// * the target task must exist and be `Open` (enforced by
//   [`crate::apply_task_assignment`]).

use soroban_sdk::xdr::ToXdr;
use soroban_sdk::unwrap::UnwrapOptimized;
use soroban_sdk::{contracttype, Address, Bytes, BytesN, Env};

use crate::{events, pausable, DataKey};

// ============================================================================
// Types
// ============================================================================

/// Structured, replay-protected authorization for a gasless task assignment.
/// Signed off-chain by the contributor, re-encoded and verified on-chain.
#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(any(test, kani), derive(Debug))]
pub struct GaslessAssignment {
    /// Address of the contract the request is bound to (domain separation).
    pub contract: Address,
    /// Task to assign.
    pub task_id: u32,
    /// Address that will be recorded as the task assignee.
    pub contributor: Address,
    /// Expected current nonce for `contributor`; consumed on success.
    pub nonce: u64,
    /// Ledger sequence after which the request is no longer valid.
    pub expiration_ledger: u32,
}

#[contracttype]
pub enum GaslessKey {
    /// Registered Ed25519 public key for a contributor address.
    SigningKey(Address),
    /// Next expected gasless-assignment nonce for a contributor address.
    Nonce(Address),
}

// ============================================================================
// Signing-key registration
// ============================================================================

/// Contributor self-registers (or rotates) the Ed25519 public key that will
/// authorize their gasless task assignments.
pub fn register_signing_key(env: Env, contributor: Address, public_key: BytesN<32>) {
    contributor.require_auth();
    env.storage()
        .instance()
        .set(&GaslessKey::SigningKey(contributor.clone()), &public_key);
    events::emit_signing_key_registered(&env, contributor.clone(), contributor);
}

/// Admin registers (or rotates) a contributor's Ed25519 public key on their
/// behalf, so a contributor can be onboarded without ever paying gas.
pub fn admin_set_signing_key(
    env: Env,
    admin: Address,
    contributor: Address,
    public_key: BytesN<32>,
) {
    admin.require_auth();
    let stored_admin: Address = env
        .storage()
        .instance()
        .get(&DataKey::Admin)
        .unwrap_optimized();
    if admin != stored_admin {
        panic!();
    }
    env.storage()
        .instance()
        .set(&GaslessKey::SigningKey(contributor.clone()), &public_key);
    events::emit_signing_key_registered(&env, contributor, admin);
}

// ============================================================================
// Views
// ============================================================================

/// The registered Ed25519 public key for `contributor`, if any.
pub fn get_signing_key(env: &Env, contributor: &Address) -> Option<BytesN<32>> {
    env.storage()
        .instance()
        .get(&GaslessKey::SigningKey(contributor.clone()))
}

/// The next nonce a `contributor` must use when signing a gasless assignment.
/// Starts at 0 and increases by one per successful gasless assignment.
pub fn get_assignment_nonce(env: &Env, contributor: &Address) -> u64 {
    env.storage()
        .instance()
        .get(&GaslessKey::Nonce(contributor.clone()))
        .unwrap_or(0)
}

/// The canonical byte string a contributor must sign for `request`. Exposed so
/// off-chain clients sign exactly what the contract will verify.
pub fn assignment_payload(env: &Env, request: GaslessAssignment) -> Bytes {
    request.to_xdr(env)
}

// ============================================================================
// Relayer endpoint
// ============================================================================

/// Relayer-submitted, signature-authorized task assignment. `relayer` is the
/// only transaction authorizer and fee payer; `request` must carry a valid
/// Ed25519 signature from `request.contributor`'s registered key.
pub fn assign_task_gasless(
    env: Env,
    relayer: Address,
    request: GaslessAssignment,
    signature: BytesN<64>,
) {
    relayer.require_auth();

    pausable::require_not_paused(
        env.clone(),
        pausable::PauseAction::AssignTask,
        Some(relayer.clone()),
    );

    // Domain separation: the signature is bound to this exact deployment.
    if request.contract != env.current_contract_address() {
        panic!();
    }

    // Time bound.
    if env.ledger().sequence() > request.expiration_ledger {
        panic!();
    }

    // Replay protection: the signed nonce must match the live one.
    let expected_nonce = get_assignment_nonce(&env, &request.contributor);
    if request.nonce != expected_nonce {
        panic!();
    }

    // Signature check against the contributor's registered key.
    let public_key = match get_signing_key(&env, &request.contributor) {
        Some(key) => key,
        None => panic!(),
    };
    let message = request.clone().to_xdr(&env);
    env.crypto().ed25519_verify(&public_key, &message, &signature);

    // Consume the nonce before doing the assignment (effects before the
    // cross-module call).
    env.storage().instance().set(
        &GaslessKey::Nonce(request.contributor.clone()),
        &(expected_nonce + 1),
    );

    crate::apply_task_assignment(&env, request.task_id, request.contributor.clone());

    events::emit_gasless_assignment(
        &env,
        request.task_id,
        request.contributor,
        relayer,
        expected_nonce,
    );
}
