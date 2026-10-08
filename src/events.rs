use soroban_sdk::{Address, BytesN, Env, String, Symbol};

// Central registry of the 58 existing event kinds. These short symbols are
// the public indexing wire format and must remain stable for existing clients.
pub mod topics {
    use soroban_sdk::{symbol_short, Symbol};

    pub const AUD_ROOT: Symbol = symbol_short!("aud_root");
    pub const TASK_CRE: Symbol = symbol_short!("task_cre");
    pub const TASK_ASSG: Symbol = symbol_short!("task_assg");
    pub const TASK_SUBM: Symbol = symbol_short!("task_subm");
    pub const TASK_COMP: Symbol = symbol_short!("task_comp");
    pub const TASK_CANC: Symbol = symbol_short!("task_canc");
    pub const TASK_DISP: Symbol = symbol_short!("task_disp");
    pub const DISP_RESL: Symbol = symbol_short!("disp_resl");
    pub const DISP_SPLT: Symbol = symbol_short!("disp_splt");
    pub const PROF_CRE: Symbol = symbol_short!("prof_cre");
    pub const PROF_UPD: Symbol = symbol_short!("prof_upd");
    pub const REP_AWARD: Symbol = symbol_short!("rep_award");
    pub const MILE_CRE: Symbol = symbol_short!("mile_cre");
    pub const MILE_SUBM: Symbol = symbol_short!("mile_subm");
    pub const MILE_APPR: Symbol = symbol_short!("mile_appr");
    pub const MILE_REJ: Symbol = symbol_short!("mile_rej");
    pub const PROP_CRE: Symbol = symbol_short!("prop_cre");
    pub const VOTE_CAST: Symbol = symbol_short!("vote_cast");
    pub const PROP_EXEC: Symbol = symbol_short!("prop_exec");
    pub const MS_CFG: Symbol = symbol_short!("ms_cfg");
    pub const MS_PROP: Symbol = symbol_short!("ms_prop");
    pub const MS_VOTE: Symbol = symbol_short!("ms_vote");
    pub const MS_EXEC: Symbol = symbol_short!("ms_exec");
    pub const MS_CANCL: Symbol = symbol_short!("ms_cancl");
    pub const ROLE_GR: Symbol = symbol_short!("role_gr");
    pub const ROLE_REV: Symbol = symbol_short!("role_rev");
    pub const PAUSED: Symbol = symbol_short!("paused");
    pub const UNPAUSED: Symbol = symbol_short!("unpaused");
    pub const LOCK: Symbol = symbol_short!("lock");
    pub const RELEASE: Symbol = symbol_short!("release");
    pub const FEE_UPD: Symbol = symbol_short!("fee_upd");
    pub const INIT: Symbol = symbol_short!("init");
    pub const RTR_CFG: Symbol = symbol_short!("rtr_cfg");
    pub const STBL_ADD: Symbol = symbol_short!("stbl_add");
    pub const STBL_REM: Symbol = symbol_short!("stbl_rem");
    pub const SWAP_EXEC: Symbol = symbol_short!("swap_exec");
    pub const SWAP_REF: Symbol = symbol_short!("swap_ref");
    pub const TOK_ADD: Symbol = symbol_short!("tok_add");
    pub const TOK_REM: Symbol = symbol_short!("tok_rem");
    pub const VLT_DEP: Symbol = symbol_short!("vlt_dep");
    pub const VLT_CLM: Symbol = symbol_short!("vlt_clm");
    pub const UPG_PROP: Symbol = symbol_short!("upg_prop");
    pub const UPG_EXEC: Symbol = symbol_short!("upg_exec");
    pub const UPG_VETO: Symbol = symbol_short!("upg_veto");
    pub const UPG_TL: Symbol = symbol_short!("upg_tl");
    pub const VV_NEW: Symbol = symbol_short!("vv_new");
    pub const VV_DISP: Symbol = symbol_short!("vv_disp");
    pub const VV_REL: Symbol = symbol_short!("vv_rel");
    pub const VV_REF: Symbol = symbol_short!("vv_ref");
    pub const TRS_FUND: Symbol = symbol_short!("trs_fund");
    pub const VST_NEW: Symbol = symbol_short!("vst_new");
    pub const VST_CLM: Symbol = symbol_short!("vst_clm");
    pub const GL_KEY: Symbol = symbol_short!("gl_key");
    pub const GL_ASSIGN: Symbol = symbol_short!("gl_assign");
    pub const REC_PROP: Symbol = symbol_short!("rec_prop");
    pub const REC_APPR: Symbol = symbol_short!("rec_appr");
    pub const REC_EXEC: Symbol = symbol_short!("rec_exec");
    pub const REC_CANCL: Symbol = symbol_short!("rec_cancl");
}

// Enforce a Symbol-kind plus one indexed entity key at every call site.
// The indexed key may be composite. This preserves existing Soroban wire shape.
macro_rules! publish_indexed {
    ($env:expr, ($kind:expr, $entity:expr), $data:expr $(,)?) => {
        $env.events().publish(($kind, $entity), $data)
    };
}

// ── Audit Log Events ─────────────────────────────────────────────────────────
pub fn emit_audit_root_recorded(
    env: &Env,
    operation: Symbol,
    subject_id: u32,
    root_hash: BytesN<32>,
    index: u32,
) {
    publish_indexed!(env,
        (topics::AUD_ROOT, subject_id),
        (operation, root_hash, index, env.ledger().timestamp()),
    );
}

// ── Task Events ────────────────────────────────────────────────────────────
pub fn emit_task_created(env: &Env, task_id: u32, creator: Address, title: Symbol, reward: i128) {
    let ledger_ts = env.ledger().timestamp();
    publish_indexed!(env,
        (topics::TASK_CRE, task_id),
        (creator, title, reward, ledger_ts),
    );
}

pub fn emit_task_assigned(env: &Env, task_id: u32, assignee: Address) {
    publish_indexed!(env,
        (topics::TASK_ASSG, task_id),
        (assignee, env.ledger().timestamp()),
    );
}

pub fn emit_task_submitted(env: &Env, task_id: u32, assignee: Address, delivery_url: Symbol) {
    publish_indexed!(env,
        (topics::TASK_SUBM, task_id),
        (assignee, delivery_url, env.ledger().timestamp()),
    );
}

pub fn emit_task_completed(env: &Env, task_id: u32, assignee: Address, payout: i128, fee: i128) {
    publish_indexed!(env,
        (topics::TASK_COMP, task_id),
        (assignee, payout, fee, env.ledger().timestamp()),
    );
}

pub fn emit_task_cancelled(env: &Env, task_id: u32, creator: Address, refund: i128) {
    publish_indexed!(env,
        (topics::TASK_CANC, task_id),
        (creator, refund, env.ledger().timestamp()),
    );
}

pub fn emit_task_disputed(env: &Env, task_id: u32, caller: Address) {
    publish_indexed!(env,
        (topics::TASK_DISP, task_id),
        (caller, env.ledger().timestamp()),
    );
}

pub fn emit_dispute_resolved(env: &Env, task_id: u32, creator_refund: i128, assignee_payout: i128) {
    publish_indexed!(env,
        (topics::DISP_RESL, task_id),
        (creator_refund, assignee_payout, env.ledger().timestamp()),
    );
}

pub fn emit_dispute_split_resolved(env: &Env, task_id: u32, platform_fee: i128, distributable: i128) {
    publish_indexed!(env,
        (topics::DISP_SPLT, task_id),
        (platform_fee, distributable, env.ledger().timestamp()),
    );
}

// ── Profile Events ─────────────────────────────────────────────────────────

pub fn emit_profile_created(env: &Env, user: Address, username: Symbol) {
    publish_indexed!(env,
        (topics::PROF_CRE, user),
        (username, env.ledger().timestamp()),
    );
}

pub fn emit_profile_updated(env: &Env, user: Address, field: Symbol) {
    publish_indexed!(env,
        (topics::PROF_UPD, user),
        (field, env.ledger().timestamp()),
    );
}

pub fn emit_reputation_awarded(env: &Env, user: Address, points: u32, new_total: u32) {
    publish_indexed!(env,
        (topics::REP_AWARD, user),
        (points, new_total, env.ledger().timestamp()),
    );
}

// ── Milestone Events ───────────────────────────────────────────────────────

pub fn emit_milestone_created(env: &Env, task_id: u32, milestone_id: u32, amount: i128) {
    publish_indexed!(env,
        (topics::MILE_CRE, (task_id, milestone_id)),
        (amount, env.ledger().timestamp()),
    );
}

pub fn emit_milestone_submitted(env: &Env, task_id: u32, milestone_id: u32, assignee: Address) {
    publish_indexed!(env,
        (topics::MILE_SUBM, (task_id, milestone_id)),
        (assignee, env.ledger().timestamp()),
    );
}

pub fn emit_milestone_approved(env: &Env, task_id: u32, milestone_id: u32, amount: i128) {
    publish_indexed!(env,
        (topics::MILE_APPR, (task_id, milestone_id)),
        (amount, env.ledger().timestamp()),
    );
}

pub fn emit_milestone_rejected(env: &Env, task_id: u32, milestone_id: u32, feedback: Symbol) {
    publish_indexed!(env,
        (topics::MILE_REJ, (task_id, milestone_id)),
        (feedback, env.ledger().timestamp()),
    );
}

// ── Governance Events ──────────────────────────────────────────────────────

pub fn emit_proposal_created(env: &Env, proposal_id: u32, proposer: Address, title: Symbol) {
    publish_indexed!(env,
        (topics::PROP_CRE, proposal_id),
        (proposer, title, env.ledger().timestamp()),
    );
}

pub fn emit_vote_cast(env: &Env, proposal_id: u32, voter: Address, vote_type: Symbol, weight: u32) {
    publish_indexed!(env,
        (topics::VOTE_CAST, (proposal_id, voter)),
        (vote_type, weight, env.ledger().timestamp()),
    );
}

pub fn emit_proposal_executed(env: &Env, proposal_id: u32, passed: bool) {
    publish_indexed!(env,
        (topics::PROP_EXEC, proposal_id),
        (passed, env.ledger().timestamp()),
    );
}

// ── Multisig Events ────────────────────────────────────────────────────────
//
// Emitted by the admin multisig ledger (`multisig.rs`). Kept distinct from the
// `prop_*` governance topics above so off-chain indexers can separate
// community proposals from privileged admin transactions.

pub fn emit_multisig_configured(env: &Env, admin: Address, signer_count: u32, threshold: u32) {
    publish_indexed!(env,
        (topics::MS_CFG, admin),
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
    publish_indexed!(env,
        (topics::MS_PROP, proposal_id),
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
    publish_indexed!(env,
        (topics::MS_VOTE, (proposal_id, signer)),
        (approvals, threshold, env.ledger().timestamp()),
    );
}

pub fn emit_multisig_executed(env: &Env, proposal_id: u32, executor: Address) {
    publish_indexed!(env,
        (topics::MS_EXEC, proposal_id),
        (executor, env.ledger().timestamp()),
    );
}

pub fn emit_multisig_cancelled(env: &Env, proposal_id: u32, caller: Address) {
    publish_indexed!(env,
        (topics::MS_CANCL, proposal_id),
        (caller, env.ledger().timestamp()),
    );
}

// ── Access Control Events ──────────────────────────────────────────────────

pub fn emit_role_granted(env: &Env, user: Address, role: Symbol, granted_by: Address) {
    publish_indexed!(env,
        (topics::ROLE_GR, user),
        (role, granted_by, env.ledger().timestamp()),
    );
}

pub fn emit_role_revoked(env: &Env, user: Address, role: Symbol, revoked_by: Address) {
    publish_indexed!(env,
        (topics::ROLE_REV, user),
        (role, revoked_by, env.ledger().timestamp()),
    );
}

// ── Pause Events ───────────────────────────────────────────────────────────

pub fn emit_paused(env: &Env, action: Symbol, admin: Address) {
    publish_indexed!(env,
        (topics::PAUSED, action),
        (admin, env.ledger().timestamp()),
    );
}

pub fn emit_unpaused(env: &Env, action: Symbol, admin: Address) {
    publish_indexed!(env,
        (topics::UNPAUSED, action),
        (admin, env.ledger().timestamp()),
    );
}

// ── Transfer Events ────────────────────────────────────────────────────────

pub fn emit_tokens_locked(env: &Env, task_id: u32, from: Address, amount: i128) {
    publish_indexed!(env,
        (topics::LOCK, task_id),
        (from, amount, env.ledger().timestamp()),
    );
}

pub fn emit_tokens_released(env: &Env, task_id: u32, to: Address, amount: i128) {
    publish_indexed!(env,
        (topics::RELEASE, task_id),
        (to, amount, env.ledger().timestamp()),
    );
}

// ── Platform Events ────────────────────────────────────────────────────────

pub fn emit_fee_updated(env: &Env, old_fee_bps: u32, new_fee_bps: u32, updated_by: Address) {
    publish_indexed!(env,
        (topics::FEE_UPD, updated_by),
        (old_fee_bps, new_fee_bps, env.ledger().timestamp()),
    );
}

pub fn emit_contract_initialized(env: &Env, admin: Address, fee_bps: u32) {
    publish_indexed!(env,
        (topics::INIT, admin),
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
    publish_indexed!(env,
        (topics::RTR_CFG, admin),
        (
            oracle,
            max_hops,
            default_slippage_bps,
            env.ledger().timestamp(),
        ),
    );
}

pub fn emit_stablecoin_approved(env: &Env, admin: Address, stablecoin: Address) {
    publish_indexed!(env,
        (topics::STBL_ADD, admin),
        (stablecoin, env.ledger().timestamp()),
    );
}

pub fn emit_stablecoin_removed(env: &Env, admin: Address, stablecoin: Address) {
    publish_indexed!(env,
        (topics::STBL_REM, admin),
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
    publish_indexed!(env,
        (topics::SWAP_EXEC, sender),
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
    publish_indexed!(env,
        (topics::SWAP_REF, sender),
        (token_in, amount, reason, env.ledger().timestamp()),
    );
}

// ── Vault Events ───────────────────────────────────────────────────────────

pub fn emit_token_supported(env: &Env, token: Address, admin: Address) {
    publish_indexed!(env,
        (topics::TOK_ADD, token),
        (admin, env.ledger().timestamp()),
    );
}

pub fn emit_token_unsupported(env: &Env, token: Address, admin: Address) {
    publish_indexed!(env,
        (topics::TOK_REM, token),
        (admin, env.ledger().timestamp()),
    );
}

pub fn emit_vault_deposit(env: &Env, depositor: Address, token: Address, amount: i128) {
    publish_indexed!(env,
        (topics::VLT_DEP, token),
        (depositor, amount, env.ledger().timestamp()),
    );
}

pub fn emit_vault_claim(env: &Env, claimant: Address, token: Address, amount: i128) {
    publish_indexed!(env,
        (topics::VLT_CLM, token),
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
    publish_indexed!(env,
        (topics::UPG_PROP, proposed_by),
        (wasm_hash, ready_at, env.ledger().timestamp()),
    );
}

pub fn emit_upgrade_executed(env: &Env, wasm_hash: soroban_sdk::BytesN<32>, executed_by: Address) {
    publish_indexed!(env,
        (topics::UPG_EXEC, executed_by),
        (wasm_hash, env.ledger().timestamp()),
    );
}

pub fn emit_upgrade_vetoed(env: &Env, wasm_hash: soroban_sdk::BytesN<32>, vetoed_by: Address) {
    publish_indexed!(env,
        (topics::UPG_VETO, vetoed_by),
        (wasm_hash, env.ledger().timestamp()),
    );
}

pub fn emit_upgrade_timelock_updated(
    env: &Env,
    old_seconds: u64,
    new_seconds: u64,
    updated_by: Address,
) {
    publish_indexed!(env,
        (topics::UPG_TL, updated_by),
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
    publish_indexed!(env,
        (topics::VV_NEW, vault_id),
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
    publish_indexed!(env,
        (topics::VV_DISP, vault_id),
        (disputed_by, reason, env.ledger().timestamp()),
    );
}

pub fn emit_vesting_vault_released(
    env: &Env,
    vault_id: u32,
    beneficiary: Address,
    amount: i128,
) {
    publish_indexed!(env,
        (topics::VV_REL, vault_id),
        (beneficiary, amount, env.ledger().timestamp()),
    );
}

pub fn emit_vesting_vault_refunded(
    env: &Env,
    vault_id: u32,
    task_creator: Address,
    amount: i128,
) {
    publish_indexed!(env,
        (topics::VV_REF, vault_id),
        (task_creator, amount, env.ledger().timestamp()),
    );
}

// ── Reward Treasury Events ─────────────────────────────────────────────────

pub fn emit_treasury_funded(env: &Env, funder: Address, amount: i128, new_balance: i128) {
    publish_indexed!(env,
        (topics::TRS_FUND, funder),
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
    publish_indexed!(env,
        (topics::VST_NEW, schedule_id),
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
    publish_indexed!(env,
        (topics::VST_CLM, schedule_id),
        (beneficiary, amount, total_claimed, env.ledger().timestamp()),
    );
}

// ── Gasless Assignment Events ─────────────────────────────────────────────

pub fn emit_signing_key_registered(env: &Env, contributor: Address, set_by: Address) {
    publish_indexed!(env,
        (topics::GL_KEY, contributor),
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
    publish_indexed!(env,
        (topics::GL_ASSIGN, task_id),
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
    publish_indexed!(env,
        (topics::REC_PROP, recovery_id),
        (proposer, proposed_admin, env.ledger().timestamp()),
    );
}

pub fn emit_recovery_approved(env: &Env, recovery_id: u32, guardian: Address, status: u32) {
    publish_indexed!(env,
        (topics::REC_APPR, (recovery_id, guardian)),
        (status, env.ledger().timestamp()),
    );
}

pub fn emit_recovery_executed(
    env: &Env,
    recovery_id: u32,
    old_admin: Address,
    new_admin: Address,
) {
    publish_indexed!(env,
        (topics::REC_EXEC, recovery_id),
        (old_admin, new_admin, env.ledger().timestamp()),
    );
}

pub fn emit_recovery_cancelled(env: &Env, recovery_id: u32, caller: Address) {
    publish_indexed!(env,
        (topics::REC_CANCL, recovery_id),
        (caller, env.ledger().timestamp()),
    );
}
