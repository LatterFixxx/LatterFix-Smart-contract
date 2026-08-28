use soroban_sdk::{symbol_short, Address, BytesN, Env, String, Symbol};

// ── Audit Log Events ─────────────────────────────────────────────────────────
pub fn emit_audit_root_recorded(
    env: &Env,
    operation: Symbol,
    subject_id: u32,
    root_hash: BytesN<32>,
    index: u32,
) {
    env.events().publish(
        (symbol_short!("aud_root"), subject_id),
        (operation, root_hash, index, env.ledger().timestamp()),
    );
}

// ── Task Events ────────────────────────────────────────────────────────────
pub fn emit_task_created(env: &Env, task_id: u32, creator: Address, title: Symbol, reward: i128) {
    let ledger_ts = env.ledger().timestamp();
    env.events().publish(
        (symbol_short!("task_cre"), task_id),
        (creator, title, reward, ledger_ts),
    );
}

pub fn emit_task_assigned(env: &Env, task_id: u32, assignee: Address) {
    env.events().publish(
        (symbol_short!("task_assg"), task_id),
        (assignee, env.ledger().timestamp()),
    );
}

pub fn emit_task_submitted(env: &Env, task_id: u32, assignee: Address, delivery_url: Symbol) {
    env.events().publish(
        (symbol_short!("task_subm"), task_id),
        (assignee, delivery_url, env.ledger().timestamp()),
    );
}

pub fn emit_task_completed(env: &Env, task_id: u32, assignee: Address, payout: i128, fee: i128) {
    env.events().publish(
        (symbol_short!("task_comp"), task_id),
        (assignee, payout, fee, env.ledger().timestamp()),
    );
}

pub fn emit_task_cancelled(env: &Env, task_id: u32, creator: Address, refund: i128) {
    env.events().publish(
        (symbol_short!("task_canc"), task_id),
        (creator, refund, env.ledger().timestamp()),
    );
}

pub fn emit_task_disputed(env: &Env, task_id: u32, caller: Address) {
    env.events().publish(
        (symbol_short!("task_disp"), task_id),
        (caller, env.ledger().timestamp()),
    );
}

pub fn emit_dispute_resolved(env: &Env, task_id: u32, creator_refund: i128, assignee_payout: i128) {
    env.events().publish(
        (symbol_short!("disp_resl"), task_id),
        (creator_refund, assignee_payout, env.ledger().timestamp()),
    );
}

pub fn emit_dispute_split_resolved(env: &Env, task_id: u32, platform_fee: i128, distributable: i128) {
    env.events().publish(
        (symbol_short!("disp_splt"), task_id),
        (platform_fee, distributable, env.ledger().timestamp()),
    );
}

// ── Profile Events ─────────────────────────────────────────────────────────

pub fn emit_profile_created(env: &Env, user: Address, username: Symbol) {
    env.events().publish(
        (symbol_short!("prof_cre"), user),
        (username, env.ledger().timestamp()),
    );
}

pub fn emit_profile_updated(env: &Env, user: Address, field: Symbol) {
    env.events().publish(
        (symbol_short!("prof_upd"), user),
        (field, env.ledger().timestamp()),
    );
}

pub fn emit_reputation_awarded(env: &Env, user: Address, points: u32, new_total: u32) {
    env.events().publish(
        (symbol_short!("rep_award"), user),
        (points, new_total, env.ledger().timestamp()),
    );
}

// ── Milestone Events ───────────────────────────────────────────────────────

pub fn emit_milestone_created(env: &Env, task_id: u32, milestone_id: u32, amount: i128) {
    env.events().publish(
        (symbol_short!("mile_cre"), (task_id, milestone_id)),
        (amount, env.ledger().timestamp()),
    );
}

pub fn emit_milestone_submitted(env: &Env, task_id: u32, milestone_id: u32, assignee: Address) {
    env.events().publish(
        (symbol_short!("mile_subm"), (task_id, milestone_id)),
        (assignee, env.ledger().timestamp()),
    );
}

pub fn emit_milestone_approved(env: &Env, task_id: u32, milestone_id: u32, amount: i128) {
    env.events().publish(
        (symbol_short!("mile_appr"), (task_id, milestone_id)),
        (amount, env.ledger().timestamp()),
    );
}

pub fn emit_milestone_rejected(env: &Env, task_id: u32, milestone_id: u32, feedback: Symbol) {
    env.events().publish(
        (symbol_short!("mile_rej"), (task_id, milestone_id)),
        (feedback, env.ledger().timestamp()),
    );
}

// ── Governance Events ──────────────────────────────────────────────────────

pub fn emit_proposal_created(env: &Env, proposal_id: u32, proposer: Address, title: Symbol) {
    env.events().publish(
        (symbol_short!("prop_cre"), proposal_id),
        (proposer, title, env.ledger().timestamp()),
    );
}

pub fn emit_vote_cast(env: &Env, proposal_id: u32, voter: Address, vote_type: Symbol, weight: u32) {
    env.events().publish(
        (symbol_short!("vote_cast"), (proposal_id, voter)),
        (vote_type, weight, env.ledger().timestamp()),
    );
}

pub fn emit_proposal_executed(env: &Env, proposal_id: u32, passed: bool) {
    env.events().publish(
        (symbol_short!("prop_exec"), proposal_id),
        (passed, env.ledger().timestamp()),
    );
}

// ── Multisig Events ────────────────────────────────────────────────────────
//
// Emitted by the admin multisig ledger (`multisig.rs`). Kept distinct from the
// `prop_*` governance topics above so off-chain indexers can separate
// community proposals from privileged admin transactions.

pub fn emit_multisig_configured(env: &Env, admin: Address, signer_count: u32, threshold: u32) {
    env.events().publish(
        (symbol_short!("ms_cfg"), admin),
        (signer_count, threshold, env.ledger().timestamp()),
    );
}

pub fn emit_multisig_proposed(
    env: &Env,
    proposal_id: u32,
    proposer: Address,
    description: Symbol,
    threshold: u32,
) {
    env.events().publish(
        (symbol_short!("ms_prop"), proposal_id),
        (proposer, description, threshold, env.ledger().timestamp()),
    );
}

pub fn emit_multisig_approved(
    env: &Env,
    proposal_id: u32,
    signer: Address,
    approvals: u32,
    threshold: u32,
) {
    env.events().publish(
        (symbol_short!("ms_vote"), (proposal_id, signer)),
        (approvals, threshold, env.ledger().timestamp()),
    );
}

pub fn emit_multisig_executed(env: &Env, proposal_id: u32, executor: Address) {
    env.events().publish(
        (symbol_short!("ms_exec"), proposal_id),
        (executor, env.ledger().timestamp()),
    );
}

pub fn emit_multisig_cancelled(env: &Env, proposal_id: u32, caller: Address) {
    env.events().publish(
        (symbol_short!("ms_cancl"), proposal_id),
        (caller, env.ledger().timestamp()),
    );
}

// ── Access Control Events ──────────────────────────────────────────────────

pub fn emit_role_granted(env: &Env, user: Address, role: Symbol, granted_by: Address) {
    env.events().publish(
        (symbol_short!("role_gr"), user),
        (role, granted_by, env.ledger().timestamp()),
    );
}

pub fn emit_role_revoked(env: &Env, user: Address, role: Symbol, revoked_by: Address) {
    env.events().publish(
        (symbol_short!("role_rev"), user),
        (role, revoked_by, env.ledger().timestamp()),
    );
}

// ── Pause Events ───────────────────────────────────────────────────────────

pub fn emit_paused(env: &Env, action: Symbol, admin: Address) {
    env.events().publish(
        (symbol_short!("paused"), action),
        (admin, env.ledger().timestamp()),
    );
}

pub fn emit_unpaused(env: &Env, action: Symbol, admin: Address) {
    env.events().publish(
        (symbol_short!("unpaused"), action),
        (admin, env.ledger().timestamp()),
    );
}

// ── Transfer Events ────────────────────────────────────────────────────────

pub fn emit_tokens_locked(env: &Env, task_id: u32, from: Address, amount: i128) {
    env.events().publish(
        (symbol_short!("lock"), task_id),
        (from, amount, env.ledger().timestamp()),
    );
}

pub fn emit_tokens_released(env: &Env, task_id: u32, to: Address, amount: i128) {
    env.events().publish(
        (symbol_short!("release"), task_id),
        (to, amount, env.ledger().timestamp()),
    );
}

// ── Platform Events ────────────────────────────────────────────────────────

pub fn emit_fee_updated(env: &Env, old_fee_bps: u32, new_fee_bps: u32, updated_by: Address) {
    env.events().publish(
        (symbol_short!("fee_upd"), updated_by),
        (old_fee_bps, new_fee_bps, env.ledger().timestamp()),
    );
}

pub fn emit_contract_initialized(env: &Env, admin: Address, fee_bps: u32) {
    env.events().publish(
        (symbol_short!("init"), admin),
        (fee_bps, env.ledger().timestamp()),
    );
}

// ── Swap Router Events ─────────────────────────────────────────────────────

pub fn emit_router_configured(
    env: &Env,
    admin: Address,
    oracle: Address,
    max_hops: u32,
    default_slippage_bps: u32,
) {
    env.events().publish(
        (symbol_short!("rtr_cfg"), admin),
        (
            oracle,
            max_hops,
            default_slippage_bps,
            env.ledger().timestamp(),
        ),
    );
}

pub fn emit_stablecoin_approved(env: &Env, admin: Address, stablecoin: Address) {
    env.events().publish(
        (symbol_short!("stbl_add"), admin),
        (stablecoin, env.ledger().timestamp()),
    );
}

pub fn emit_stablecoin_removed(env: &Env, admin: Address, stablecoin: Address) {
    env.events().publish(
        (symbol_short!("stbl_rem"), admin),
        (stablecoin, env.ledger().timestamp()),
    );
}

pub fn emit_swap_executed(
    env: &Env,
    sender: Address,
    token_in: Address,
    token_out: Address,
    amount_in: i128,
    amount_out: i128,
) {
    env.events().publish(
        (symbol_short!("swap_exec"), sender),
        (
            token_in,
            token_out,
            amount_in,
            amount_out,
            env.ledger().timestamp(),
        ),
    );
}

pub fn emit_swap_refunded(
    env: &Env,
    sender: Address,
    token_in: Address,
    amount: i128,
    reason: Symbol,
) {
    env.events().publish(
        (symbol_short!("swap_ref"), sender),
        (token_in, amount, reason, env.ledger().timestamp()),
    );
}

// ── Vault Events ───────────────────────────────────────────────────────────

pub fn emit_token_supported(env: &Env, token: Address, admin: Address) {
    env.events().publish(
        (symbol_short!("tok_add"), token),
        (admin, env.ledger().timestamp()),
    );
}

pub fn emit_token_unsupported(env: &Env, token: Address, admin: Address) {
    env.events().publish(
        (symbol_short!("tok_rem"), token),
        (admin, env.ledger().timestamp()),
    );
}

pub fn emit_vault_deposit(env: &Env, depositor: Address, token: Address, amount: i128) {
    env.events().publish(
        (symbol_short!("vlt_dep"), token),
        (depositor, amount, env.ledger().timestamp()),
    );
}

pub fn emit_vault_claim(env: &Env, claimant: Address, token: Address, amount: i128) {
    env.events().publish(
        (symbol_short!("vlt_clm"), token),
        (claimant, amount, env.ledger().timestamp()),
    );
}

// ── Upgrade Timelock Events ────────────────────────────────────────────────

pub fn emit_upgrade_proposed(
    env: &Env,
    wasm_hash: soroban_sdk::BytesN<32>,
    proposed_by: Address,
    ready_at: u64,
) {
    env.events().publish(
        (symbol_short!("upg_prop"), proposed_by),
        (wasm_hash, ready_at, env.ledger().timestamp()),
    );
}

pub fn emit_upgrade_executed(env: &Env, wasm_hash: soroban_sdk::BytesN<32>, executed_by: Address) {
    env.events().publish(
        (symbol_short!("upg_exec"), executed_by),
        (wasm_hash, env.ledger().timestamp()),
    );
}

pub fn emit_upgrade_vetoed(env: &Env, wasm_hash: soroban_sdk::BytesN<32>, vetoed_by: Address) {
    env.events().publish(
        (symbol_short!("upg_veto"), vetoed_by),
        (wasm_hash, env.ledger().timestamp()),
    );
}

pub fn emit_upgrade_timelock_updated(
    env: &Env,
    old_seconds: u64,
    new_seconds: u64,
    updated_by: Address,
) {
    env.events().publish(
        (symbol_short!("upg_tl"), updated_by),
        (old_seconds, new_seconds, env.ledger().timestamp()),
    );
}

// ── Vesting Vault Events ───────────────────────────────────────────────────

pub fn emit_vesting_vault_created(
    env: &Env,
    vault_id: u32,
    task_id: u32,
    milestone_id: u32,
    beneficiary: Address,
    amount: i128,
    vesting_end: u64,
) {
    env.events().publish(
        (symbol_short!("vv_new"), vault_id),
        (
            task_id,
            milestone_id,
            beneficiary,
            amount,
            vesting_end,
            env.ledger().timestamp(),
        ),
    );
}

pub fn emit_vesting_vault_disputed(
    env: &Env,
    vault_id: u32,
    disputed_by: Address,
    reason: String,
) {
    env.events().publish(
        (symbol_short!("vv_disp"), vault_id),
        (disputed_by, reason, env.ledger().timestamp()),
    );
}

pub fn emit_vesting_vault_released(
    env: &Env,
    vault_id: u32,
    beneficiary: Address,
    amount: i128,
) {
    env.events().publish(
        (symbol_short!("vv_rel"), vault_id),
        (beneficiary, amount, env.ledger().timestamp()),
    );
}

pub fn emit_vesting_vault_refunded(
    env: &Env,
    vault_id: u32,
    task_creator: Address,
    amount: i128,
) {
    env.events().publish(
        (symbol_short!("vv_ref"), vault_id),
        (task_creator, amount, env.ledger().timestamp()),
    );
}

// ── Reward Treasury Events ─────────────────────────────────────────────────

pub fn emit_treasury_funded(env: &Env, funder: Address, amount: i128, new_balance: i128) {
    env.events().publish(
        (symbol_short!("trs_fund"), funder),
        (amount, new_balance, env.ledger().timestamp()),
    );
}

pub fn emit_vesting_schedule_created(
    env: &Env,
    schedule_id: u32,
    beneficiary: Address,
    total_amount: i128,
    decay_rate_bps: u32,
) {
    env.events().publish(
        (symbol_short!("vst_new"), schedule_id),
        (
            beneficiary,
            total_amount,
            decay_rate_bps,
            env.ledger().timestamp(),
        ),
    );
}

pub fn emit_vesting_claimed(
    env: &Env,
    schedule_id: u32,
    beneficiary: Address,
    amount: i128,
    total_claimed: i128,
) {
    env.events().publish(
        (symbol_short!("vst_clm"), schedule_id),
        (beneficiary, amount, total_claimed, env.ledger().timestamp()),
    );
}

// ── Gasless Assignment Events ─────────────────────────────────────────────

pub fn emit_signing_key_registered(env: &Env, contributor: Address, set_by: Address) {
    env.events().publish(
        (symbol_short!("gl_key"), contributor),
        (set_by, env.ledger().timestamp()),
    );
}

pub fn emit_gasless_assignment(
    env: &Env,
    task_id: u32,
    contributor: Address,
    relayer: Address,
    nonce: u64,
) {
    env.events().publish(
        (symbol_short!("gl_assign"), task_id),
        (contributor, relayer, nonce, env.ledger().timestamp()),
    );
}

// ── Social Recovery Events ────────────────────────────────────────────────

pub fn emit_recovery_proposed(
    env: &Env,
    recovery_id: u32,
    proposer: Address,
    proposed_admin: Address,
) {
    env.events().publish(
        (symbol_short!("rec_prop"), recovery_id),
        (proposer, proposed_admin, env.ledger().timestamp()),
    );
}

pub fn emit_recovery_approved(env: &Env, recovery_id: u32, guardian: Address, status: u32) {
    env.events().publish(
        (symbol_short!("rec_appr"), (recovery_id, guardian)),
        (status, env.ledger().timestamp()),
    );
}

pub fn emit_recovery_executed(
    env: &Env,
    recovery_id: u32,
    old_admin: Address,
    new_admin: Address,
) {
    env.events().publish(
        (symbol_short!("rec_exec"), recovery_id),
        (old_admin, new_admin, env.ledger().timestamp()),
    );
}

pub fn emit_recovery_cancelled(env: &Env, recovery_id: u32, caller: Address) {
    env.events().publish(
        (symbol_short!("rec_cancl"), recovery_id),
        (caller, env.ledger().timestamp()),
    );
}
