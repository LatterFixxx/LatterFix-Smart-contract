# Soroban event-indexer dictionary

All 58 public event types emitted by `src/events.rs` use exactly two topic positions: a stable short `Symbol` event kind, then an indexed entity key. The entity key is a `u32`, `Address`, `Symbol`, or documented composite tuple. Payload types and ordering are preserved from the current deployed API; the final field for all entries is the ledger timestamp (`u64`). The caller's contract ID is provided in Soroban's event envelope, not repeated as a topic. Indexers should filter by contract ID, kind, and only then the entity key. Do not assume composite keys are strings.

This registry intentionally avoids renaming existing kinds or flattening composite keys because doing so would silently break historical event streams. Any incompatible future migration requires a new versioned topic and dual-read period. The `publish_indexed!` helper and `topics` constants enforce consistent emitter shape without adding storage or modifying transaction behavior.

| Kind (topic 0) | Indexed entity (topic 1) | Ordered data payload | Emit helper |
|---|---|---|---|
| `aud_root` | `subject_id` | `operation, root_hash, index, ledger_timestamp` | `emit_audit_root_recorded` |
| `task_cre` | `task_id` | `creator, title, reward, ledger_ts` | `emit_task_created` |
| `task_assg` | `task_id` | `assignee, ledger_timestamp` | `emit_task_assigned` |
| `task_subm` | `task_id` | `assignee, delivery_url, ledger_timestamp` | `emit_task_submitted` |
| `task_comp` | `task_id` | `assignee, payout, fee, ledger_timestamp` | `emit_task_completed` |
| `task_canc` | `task_id` | `creator, refund, ledger_timestamp` | `emit_task_cancelled` |
| `task_disp` | `task_id` | `caller, ledger_timestamp` | `emit_task_disputed` |
| `disp_resl` | `task_id` | `creator_refund, assignee_payout, ledger_timestamp` | `emit_dispute_resolved` |
| `disp_splt` | `task_id` | `platform_fee, distributable, ledger_timestamp` | `emit_dispute_split_resolved` |
| `prof_cre` | `user` | `username, ledger_timestamp` | `emit_profile_created` |
| `prof_upd` | `user` | `field, ledger_timestamp` | `emit_profile_updated` |
| `rep_award` | `user` | `points, new_total, ledger_timestamp` | `emit_reputation_awarded` |
| `mile_cre` | `(task_id, milestone_id)` | `amount, ledger_timestamp` | `emit_milestone_created` |
| `mile_subm` | `(task_id, milestone_id)` | `assignee, ledger_timestamp` | `emit_milestone_submitted` |
| `mile_appr` | `(task_id, milestone_id)` | `amount, ledger_timestamp` | `emit_milestone_approved` |
| `mile_rej` | `(task_id, milestone_id)` | `feedback, ledger_timestamp` | `emit_milestone_rejected` |
| `prop_cre` | `proposal_id` | `proposer, title, ledger_timestamp` | `emit_proposal_created` |
| `vote_cast` | `(proposal_id, voter)` | `vote_type, weight, ledger_timestamp` | `emit_vote_cast` |
| `prop_exec` | `proposal_id` | `passed, ledger_timestamp` | `emit_proposal_executed` |
| `ms_cfg` | `admin` | `signer_count, threshold, ledger_timestamp` | `emit_multisig_configured` |
| `ms_prop` | `proposal_id` | `proposer, description, threshold, ledger_timestamp` | `emit_multisig_proposed` |
| `ms_vote` | `(proposal_id, signer)` | `approvals, threshold, ledger_timestamp` | `emit_multisig_approved` |
| `ms_exec` | `proposal_id` | `executor, ledger_timestamp` | `emit_multisig_executed` |
| `ms_cancl` | `proposal_id` | `caller, ledger_timestamp` | `emit_multisig_cancelled` |
| `role_gr` | `user` | `role, granted_by, ledger_timestamp` | `emit_role_granted` |
| `role_rev` | `user` | `role, revoked_by, ledger_timestamp` | `emit_role_revoked` |
| `paused` | `action` | `admin, ledger_timestamp` | `emit_paused` |
| `unpaused` | `action` | `admin, ledger_timestamp` | `emit_unpaused` |
| `lock` | `task_id` | `from, amount, ledger_timestamp` | `emit_tokens_locked` |
| `release` | `task_id` | `to, amount, ledger_timestamp` | `emit_tokens_released` |
| `fee_upd` | `updated_by` | `old_fee_bps, new_fee_bps, ledger_timestamp` | `emit_fee_updated` |
| `init` | `admin` | `fee_bps, ledger_timestamp` | `emit_contract_initialized` |
| `rtr_cfg` | `admin` | ` oracle, max_hops, default_slippage_bps, ledger_timestamp, ` | `emit_router_configured` |
| `stbl_add` | `admin` | `stablecoin, ledger_timestamp` | `emit_stablecoin_approved` |
| `stbl_rem` | `admin` | `stablecoin, ledger_timestamp` | `emit_stablecoin_removed` |
| `swap_exec` | `sender` | ` token_in, token_out, amount_in, amount_out, ledger_timestamp, ` | `emit_swap_executed` |
| `swap_ref` | `sender` | `token_in, amount, reason, ledger_timestamp` | `emit_swap_refunded` |
| `tok_add` | `token` | `admin, ledger_timestamp` | `emit_token_supported` |
| `tok_rem` | `token` | `admin, ledger_timestamp` | `emit_token_unsupported` |
| `vlt_dep` | `token` | `depositor, amount, ledger_timestamp` | `emit_vault_deposit` |
| `vlt_clm` | `token` | `claimant, amount, ledger_timestamp` | `emit_vault_claim` |
| `upg_prop` | `proposed_by` | `wasm_hash, ready_at, ledger_timestamp` | `emit_upgrade_proposed` |
| `upg_exec` | `executed_by` | `wasm_hash, ledger_timestamp` | `emit_upgrade_executed` |
| `upg_veto` | `vetoed_by` | `wasm_hash, ledger_timestamp` | `emit_upgrade_vetoed` |
| `upg_tl` | `updated_by` | `old_seconds, new_seconds, ledger_timestamp` | `emit_upgrade_timelock_updated` |
| `vv_new` | `vault_id` | ` task_id, milestone_id, beneficiary, amount, vesting_end, ledger_timestamp, ` | `emit_vesting_vault_created` |
| `vv_disp` | `vault_id` | `disputed_by, reason, ledger_timestamp` | `emit_vesting_vault_disputed` |
| `vv_rel` | `vault_id` | `beneficiary, amount, ledger_timestamp` | `emit_vesting_vault_released` |
| `vv_ref` | `vault_id` | `task_creator, amount, ledger_timestamp` | `emit_vesting_vault_refunded` |
| `trs_fund` | `funder` | `amount, new_balance, ledger_timestamp` | `emit_treasury_funded` |
| `vst_new` | `schedule_id` | ` beneficiary, total_amount, decay_rate_bps, ledger_timestamp, ` | `emit_vesting_schedule_created` |
| `vst_clm` | `schedule_id` | `beneficiary, amount, total_claimed, ledger_timestamp` | `emit_vesting_claimed` |
| `gl_key` | `contributor` | `set_by, ledger_timestamp` | `emit_signing_key_registered` |
| `gl_assign` | `task_id` | `contributor, relayer, nonce, ledger_timestamp` | `emit_gasless_assignment` |
| `rec_prop` | `recovery_id` | `proposer, proposed_admin, ledger_timestamp` | `emit_recovery_proposed` |
| `rec_appr` | `(recovery_id, guardian)` | `status, ledger_timestamp` | `emit_recovery_approved` |
| `rec_exec` | `recovery_id` | `old_admin, new_admin, ledger_timestamp` | `emit_recovery_executed` |
| `rec_cancl` | `recovery_id` | `caller, ledger_timestamp` | `emit_recovery_cancelled` |

Note: these are **Soroban contract events** with typed XDR values, not JSON payloads. Do not infer a distinct schema from function argument order: event data can omit or reorder emitter arguments as shown. Call `env.events().all()` in the focused contract test to validate exact encoded topic and data tuples.
