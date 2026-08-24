#![no_std]
use soroban_sdk::unwrap::UnwrapOptimized;

pub mod access_control;
pub mod benchmark;
pub mod escrow;
pub mod events;
pub mod governance;
pub mod merkle;
pub mod multisig;
pub mod pausable;
pub mod reputation;
pub mod storage;
pub mod swap_router;
pub mod treasury;
pub mod twap_oracle;
pub mod upgrade;
pub mod user_profile;
pub mod vault;
pub mod vesting_vault;
pub mod zkp_attestation;

#[cfg(kani)]
pub mod kani_proofs;

#[cfg(test)]
mod multisig_test;
#[cfg(test)]
mod swap_router_test;
#[cfg(test)]
mod test;
#[cfg(test)]
mod treasury_test;
#[cfg(test)]
mod upgrade_test;
#[cfg(test)]
mod vesting_vault_test;
#[cfg(test)]
mod batch_test;

use soroban_sdk::{contract, contractimpl, contracttype, Address, BytesN, Env, String, Symbol, Vec};

// ============================================================================
// Task Management Types
// ============================================================================

#[contracttype]
#[derive(Clone, Copy, Eq, PartialEq)]
#[cfg_attr(any(test, kani), derive(Debug))]
#[repr(u32)]
pub enum TaskStatus {
    Open = 0,
    InProgress = 1,
    Completed = 2,
    Verified = 3,
    Cancelled = 4,
    Disputed = 5,
    Resolved = 6,
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
pub struct Task {
    pub id: u32,
    pub title: Symbol,
    pub description: Symbol,
    pub reward: i128,
    pub assignee: Option<Address>,
    pub status: TaskStatus,
    pub created_by: Address,
    pub tags: Vec<Symbol>,
    pub category_id: Option<u32>,
    pub deadline: Option<u64>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
pub struct TaskWithMilestones {
    pub task: Task,
    pub milestones: Vec<escrow::Milestone>,
    pub total_milestone_amount: i128,
}

#[contracttype]
#[cfg_attr(any(test, kani), derive(Debug))]
#[derive(Clone, Eq, PartialEq)]
pub struct BatchTask {
    pub id: u32,
    pub creator: Address,
    pub merkle_root: BytesN<32>,
    pub total_reward: i128,
    pub tasks_count: u32,
}

#[contracttype]
#[cfg_attr(any(test, kani), derive(Debug))]
#[derive(Clone, Eq, PartialEq)]
pub struct BatchTaskLeaf {
    pub index: u32,
    pub title: Symbol,
    pub description: Symbol,
    pub reward: i128,
    pub assignee: Option<Address>,
}


// ============================================================================
// Storage Keys
// ============================================================================

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
pub enum DataKey {
    Admin,
    PlatformFeeBps,
    TokenContract,
    FeeRecipient,
    Initialized,
    Task(u32),
    TaskCount,
    BatchTask(u32),
    BatchTaskCount,
    BatchTaskClaimed(u32, u32),
    UserProfile(Address),
    Paused(u32),
    GovernanceConfig,
}

// ============================================================================
// Dispute Split Types
// ============================================================================

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
pub struct DisputeSplitRecipient {
    pub address: Address,
    pub share_bps: u32,
}

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
pub struct DisputeSplit {
    pub recipients: Vec<DisputeSplitRecipient>,
}

// ============================================================================
// Main Contract
// ============================================================================

#[contract]
pub struct TaskManagerContract;

#[contractimpl]
impl TaskManagerContract {
    // ========================================================================
    // Initialization
    // ========================================================================

    pub fn initialize(
        env: Env,
        admin: Address,
        platform_fee_bps: u32,
        token_contract: Address,
        fee_recipient: Address,
    ) {
        if env.storage().instance().has(&DataKey::Initialized) {
            panic!();
        }

        // Validate fee (max 10%)
        if platform_fee_bps > 1000 {
            panic!();
        }

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::PlatformFeeBps, &platform_fee_bps);
        env.storage()
            .instance()
            .set(&DataKey::TokenContract, &token_contract);
        env.storage()
            .instance()
            .set(&DataKey::FeeRecipient, &fee_recipient);
        env.storage().instance().set(&DataKey::Initialized, &true);
        env.storage().instance().set(&DataKey::TaskCount, &0u32);

        // Initialize reputation tiers
        reputation::init_reputation_tiers(env.clone());

        // Initialize governance config
        env.storage().persistent().set(
            &governance::GovernanceKey::Config,
            &governance::GovernanceConfig {
                voting_period: 604800, // 7 days
                quorum: 10,
                threshold: 51,
                min_reputation_to_propose: 100,
                min_reputation_to_vote: 50,
            },
        );
    }

    // ========================================================================
    // Task Management
    // ========================================================================


    // ========================================================================
    // Batch Task Management
    // ========================================================================

    pub fn create_batch_tasks(
        env: Env,
        creator: Address,
        merkle_root: BytesN<32>,
        total_reward: i128,
        tasks_count: u32,
    ) -> u32 {
        creator.require_auth();

        pausable::require_not_paused(
            env.clone(),
            pausable::PauseAction::CreateTask,
            Some(creator.clone()),
        );

        if total_reward <= 0 || tasks_count == 0 {
            panic!("Invalid batch parameters");
        }

        let token_contract: Address = env
            .storage()
            .instance()
            .get(&DataKey::TokenContract)
            .unwrap_optimized();

        let token_client = soroban_sdk::token::Client::new(&env, &token_contract);
        token_client.transfer(&creator, &env.current_contract_address(), &total_reward);

        let mut batch_count: u32 = env
            .storage()
            .instance()
            .get(&DataKey::BatchTaskCount)
            .unwrap_or(0);
        batch_count += 1;
        env.storage()
            .instance()
            .set(&DataKey::BatchTaskCount, &batch_count);

        let batch = BatchTask {
            id: batch_count,
            creator,
            merkle_root,
            total_reward,
            tasks_count,
        };

        env.storage()
            .instance()
            .set(&DataKey::BatchTask(batch_count), &batch);

        batch_count
    }

    pub fn verify_and_claim_batch_task(
        env: Env,
        batch_id: u32,
        leaf: BatchTaskLeaf,
        proof: Vec<BytesN<32>>,
    ) -> u32 {
        let batch: BatchTask = env
            .storage()
            .instance()
            .get(&DataKey::BatchTask(batch_id))
            .unwrap_optimized();

        let claimed_key = DataKey::BatchTaskClaimed(batch_id, leaf.index);
        if env.storage().instance().has(&claimed_key) {
            panic!("Task already claimed");
        }

        // Serialize leaf to XDR for hashing
        use soroban_sdk::xdr::ToXdr;
        let leaf_bytes = leaf.clone().to_xdr(&env);
        let leaf_hash = env.crypto().sha256(&leaf_bytes).into();

        if !crate::merkle::verify_merkle_proof(&env, &batch.merkle_root, &leaf_hash, &proof) {
            panic!("Invalid Merkle proof");
        }

        // Mark claimed
        env.storage().instance().set(&claimed_key, &true);

        // Instantiate Task
        let mut task_count: u32 = env
            .storage()
            .instance()
            .get(&DataKey::TaskCount)
            .unwrap_or(0);
        task_count += 1;
        env.storage()
            .instance()
            .set(&DataKey::TaskCount, &task_count);

        let now = env.ledger().timestamp();
        let task = Task {
            id: task_count,
            title: leaf.title.clone(),
            description: leaf.description.clone(),
            reward: leaf.reward,
            assignee: leaf.assignee,
            status: TaskStatus::Open,
            created_by: batch.creator.clone(),
            tags: soroban_sdk::vec![&env],
            category_id: None,
            deadline: None,
            created_at: now,
            updated_at: now,
        };

        env.storage()
            .instance()
            .set(&DataKey::Task(task_count), &task);

        // Lock escrow (funds are already in the contract from create_batch_tasks)
        escrow::lock_escrow(env.clone(), task_count, leaf.reward);

        events::emit_task_created(&env, task_count, batch.creator, leaf.title, leaf.reward);

        storage::update_statistics(&env, |stats| {
            stats.total_tasks_created += 1;
        });

        task_count
    }

    pub fn create_task(
        env: Env,
        creator: Address,
        title: Symbol,
        description: Symbol,
        reward: i128,
        tags: Vec<Symbol>,
    ) -> u32 {
        creator.require_auth();

        // Check if paused
        pausable::require_not_paused(
            env.clone(),
            pausable::PauseAction::CreateTask,
            Some(creator.clone()),
        );

        if reward <= 0 {
            panic!();
        }

        let token_contract: Address = env
            .storage()
            .instance()
            .get(&DataKey::TokenContract)
            .unwrap_optimized();

        // Transfer reward from creator to the contract
        let token_client = soroban_sdk::token::Client::new(&env, &token_contract);
        token_client.transfer(&creator, &env.current_contract_address(), &reward);

        let mut task_count: u32 = env
            .storage()
            .instance()
            .get(&DataKey::TaskCount)
            .unwrap_or(0);
        task_count += 1;
        env.storage()
            .instance()
            .set(&DataKey::TaskCount, &task_count);

        let now = env.ledger().timestamp();

        let task = Task {
            id: task_count,
            title: title.clone(),
            description,
            reward,
            assignee: None,
            status: TaskStatus::Open,
            created_by: creator.clone(),
            tags,
            category_id: None,
            deadline: None,
            created_at: now,
            updated_at: now,
        };

        env.storage()
            .instance()
            .set(&DataKey::Task(task_count), &task);

        // Lock escrow
        escrow::lock_escrow(env.clone(), task_count, reward);

        // Emit event
        events::emit_task_created(&env, task_count, creator, title, reward);

        // Update statistics
        storage::update_statistics(&env, |stats| {
            stats.total_tasks_created += 1;
            stats.total_value_locked += reward;
        });

        task_count
    }

    pub fn create_task_with_milestones(
        env: Env,
        creator: Address,
        title: Symbol,
        description: Symbol,
        milestones: Vec<(Symbol, i128)>, // (title, amount)
        tags: Vec<Symbol>,
    ) -> u32 {
        creator.require_auth();

        // Calculate total reward from milestones
        let mut total_reward: i128 = 0;
        for i in 0..milestones.len() {
            let milestone = milestones.get(i).unwrap_optimized();
            total_reward += milestone.1;
        }

        if total_reward <= 0 {
            panic!();
        }

        // Create the task
        let task_id = Self::create_task(
            env.clone(),
            creator.clone(),
            title,
            description,
            total_reward,
            tags,
        );

        // Create milestones
        for (milestone_title, amount) in milestones.iter() {
            escrow::create_milestone(env.clone(), task_id, milestone_title.clone(), amount, None);
        }

        task_id
    }

    pub fn assign_task(env: Env, assignee: Address, task_id: u32) {
        assignee.require_auth();

        pausable::require_not_paused(
            env.clone(),
            pausable::PauseAction::AssignTask,
            Some(assignee.clone()),
        );

        let mut task: Task = env
            .storage()
            .instance()
            .get(&DataKey::Task(task_id))
            .unwrap_optimized();

        if task.status != TaskStatus::Open {
            panic!();
        }

        task.assignee = Some(assignee.clone());
        task.status = TaskStatus::InProgress;
        task.updated_at = env.ledger().timestamp();

        env.storage().instance().set(&DataKey::Task(task_id), &task);

        events::emit_task_assigned(&env, task_id, assignee);
    }

    pub fn submit_work(env: Env, assignee: Address, task_id: u32, delivery_url: Symbol) {
        assignee.require_auth();

        pausable::require_not_paused(
            env.clone(),
            pausable::PauseAction::SubmitWork,
            Some(assignee.clone()),
        );

        let mut task: Task = env
            .storage()
            .instance()
            .get(&DataKey::Task(task_id))
            .unwrap_optimized();

        if task.assignee.as_ref() != Some(&assignee) {
            panic!();
        }

        if task.status != TaskStatus::InProgress {
            panic!();
        }

        task.status = TaskStatus::Completed;
        task.updated_at = env.ledger().timestamp();

        env.storage().instance().set(&DataKey::Task(task_id), &task);

        events::emit_task_submitted(&env, task_id, assignee, delivery_url);
    }

    pub fn complete_task(env: Env, caller: Address, task_id: u32) {
        caller.require_auth();

        pausable::require_not_paused(
            env.clone(),
            pausable::PauseAction::CompleteTask,
            Some(caller.clone()),
        );

        let mut task: Task = env
            .storage()
            .instance()
            .get(&DataKey::Task(task_id))
            .unwrap_optimized();

        if task.status != TaskStatus::Completed {
            panic!();
        }

        // Verify caller is creator or admin
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap_optimized();
        if caller != task.created_by && caller != admin {
            panic!();
        }

        let platform_fee_bps: u32 = env
            .storage()
            .instance()
            .get(&DataKey::PlatformFeeBps)
            .unwrap_or(0);
        let fee_recipient: Address = env
            .storage()
            .instance()
            .get(&DataKey::FeeRecipient)
            .unwrap_optimized();
        let token_contract: Address = env
            .storage()
            .instance()
            .get(&DataKey::TokenContract)
            .unwrap_optimized();

        let fee = (task.reward * platform_fee_bps as i128) / 10000;
        let payout = task.reward - fee;

        let token_client = soroban_sdk::token::Client::new(&env, &token_contract);

        let assignee = task
            .assignee
            .clone()
            .unwrap_optimized();

        if fee > 0 {
            token_client.transfer(&env.current_contract_address(), &fee_recipient, &fee);
        }
        if payout > 0 {
            token_client.transfer(&env.current_contract_address(), &assignee, &payout);
        }

        // Release escrow
        escrow::release_escrow(env.clone(), task_id, task.reward);

        task.status = TaskStatus::Verified;
        task.updated_at = env.ledger().timestamp();
        env.storage().instance().set(&DataKey::Task(task_id), &task);

        // Award reputation
        reputation::award_reputation(
            env.clone(),
            assignee.clone(),
            reputation::points_for_event(reputation::ReputationEventType::TaskVerified),
            reputation::ReputationEventType::TaskVerified,
            Some(task_id),
            Symbol::new(&env, "Task_verified_and_completed"),
        );

        events::emit_task_completed(&env, task_id, assignee, payout, fee);

        // Update statistics
        storage::update_statistics(&env, |stats| {
            stats.total_tasks_completed += 1;
            stats.total_value_paid += payout;
            stats.total_platform_fees += fee;
        });
    }

    pub fn cancel_task(env: Env, creator: Address, task_id: u32) {
        creator.require_auth();

        pausable::require_not_paused(
            env.clone(),
            pausable::PauseAction::CancelTask,
            Some(creator.clone()),
        );

        let mut task: Task = env
            .storage()
            .instance()
            .get(&DataKey::Task(task_id))
            .unwrap_optimized();

        if task.created_by != creator {
            panic!();
        }

        if task.status != TaskStatus::Open {
            panic!();
        }

        let token_contract: Address = env
            .storage()
            .instance()
            .get(&DataKey::TokenContract)
            .unwrap_optimized();
        let token_client = soroban_sdk::token::Client::new(&env, &token_contract);
        token_client.transfer(&env.current_contract_address(), &creator, &task.reward);

        // Release escrow (refund)
        escrow::release_escrow(env.clone(), task_id, task.reward);

        task.status = TaskStatus::Cancelled;
        task.updated_at = env.ledger().timestamp();
        env.storage().instance().set(&DataKey::Task(task_id), &task);

        events::emit_task_cancelled(&env, task_id, creator, task.reward);

        // Update statistics
        storage::update_statistics(&env, |stats| {
            stats.total_tasks_cancelled += 1;
        });
    }

    pub fn dispute_task(env: Env, caller: Address, task_id: u32) {
        caller.require_auth();

        pausable::require_not_paused(
            env.clone(),
            pausable::PauseAction::DisputeTask,
            Some(caller.clone()),
        );

        let mut task: Task = env
            .storage()
            .instance()
            .get(&DataKey::Task(task_id))
            .unwrap_optimized();

        if caller != task.created_by && Some(&caller) != task.assignee.as_ref() {
            panic!();
        }

        if task.status != TaskStatus::InProgress && task.status != TaskStatus::Completed {
            panic!();
        }

        task.status = TaskStatus::Disputed;
        task.updated_at = env.ledger().timestamp();
        env.storage().instance().set(&DataKey::Task(task_id), &task);

        events::emit_task_disputed(&env, task_id, caller);

        // Update statistics
        storage::update_statistics(&env, |stats| {
            stats.total_tasks_disputed += 1;
        });
    }

    pub fn resolve_dispute(
        env: Env,
        admin: Address,
        task_id: u32,
        creator_refund: i128,
        assignee_payout: i128,
    ) {
        admin.require_auth();

        let stored_admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap_optimized();
        if admin != stored_admin {
            panic!();
        }

        let mut task: Task = env
            .storage()
            .instance()
            .get(&DataKey::Task(task_id))
            .unwrap_optimized();

        if task.status != TaskStatus::Disputed {
            panic!();
        }

        if creator_refund + assignee_payout != task.reward {
            panic!();
        }

        let token_contract: Address = env
            .storage()
            .instance()
            .get(&DataKey::TokenContract)
            .unwrap_optimized();
        let token_client = soroban_sdk::token::Client::new(&env, &token_contract);

        if creator_refund > 0 {
            token_client.transfer(
                &env.current_contract_address(),
                &task.created_by,
                &creator_refund,
            );
        }
        if assignee_payout > 0 {
            let assignee = task
                .assignee
                .clone()
                .unwrap_optimized();
            token_client.transfer(&env.current_contract_address(), &assignee, &assignee_payout);

            // Award reputation for winning dispute
            reputation::award_reputation(
                env.clone(),
                assignee.clone(),
                reputation::points_for_event(reputation::ReputationEventType::DisputeWon),
                reputation::ReputationEventType::DisputeWon,
                Some(task_id),
                Symbol::new(&env, "Won_dispute"),
            );
        }

        // Release escrow
        escrow::release_escrow(env.clone(), task_id, task.reward);

        task.status = TaskStatus::Resolved;
        task.updated_at = env.ledger().timestamp();
        env.storage().instance().set(&DataKey::Task(task_id), &task);

        events::emit_dispute_resolved(&env, task_id, creator_refund, assignee_payout);
    }

    pub fn resolve_dispute_split(
        env: Env,
        task_id: u32,
        recipients: Vec<Address>,
        shares_bps: Vec<u32>,
    ) {
        crate::resolve_dispute_split(env, task_id, recipients, shares_bps);
    }

    // ========================================================================
    // Milestone Management
    // ========================================================================

    pub fn submit_milestone(
        env: Env,
        assignee: Address,
        task_id: u32,
        milestone_id: u32,
        submission_url: Symbol,
    ) {
        assignee.require_auth();

        // Verify assignee
        let task: Task = env
            .storage()
            .instance()
            .get(&DataKey::Task(task_id))
            .unwrap_optimized();

        if task.assignee.as_ref() != Some(&assignee) {
            panic!();
        }

        escrow::submit_milestone(env.clone(), task_id, milestone_id, submission_url);

        events::emit_milestone_submitted(&env, task_id, milestone_id, assignee);
    }

    pub fn approve_milestone(
        env: Env,
        caller: Address,
        task_id: u32,
        milestone_id: u32,
        feedback: Option<Symbol>,
    ) {
        caller.require_auth();

        let task: Task = env
            .storage()
            .instance()
            .get(&DataKey::Task(task_id))
            .unwrap_optimized();

        // Only creator or admin can approve
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap_optimized();
        if caller != task.created_by && caller != admin {
            panic!();
        }

        let amount =
            escrow::approve_milestone(env.clone(), task_id, milestone_id, feedback.clone());

        // Transfer milestone payment to assignee
        let token_contract: Address = env
            .storage()
            .instance()
            .get(&DataKey::TokenContract)
            .unwrap_optimized();
        let token_client = soroban_sdk::token::Client::new(&env, &token_contract);
        let assignee = task.assignee.clone().unwrap_optimized();

        token_client.transfer(&env.current_contract_address(), &assignee, &amount);

        events::emit_milestone_approved(&env, task_id, milestone_id, amount);

        // Award reputation
        reputation::award_reputation(
            env.clone(),
            assignee.clone(),
            reputation::points_for_event(reputation::ReputationEventType::MilestoneApproved),
            reputation::ReputationEventType::MilestoneApproved,
            Some(task_id),
            Symbol::new(&env, "Milestone_approved"),
        );
    }

    pub fn reject_milestone(
        env: Env,
        caller: Address,
        task_id: u32,
        milestone_id: u32,
        feedback: Symbol,
    ) {
        caller.require_auth();

        let task: Task = env
            .storage()
            .instance()
            .get(&DataKey::Task(task_id))
            .unwrap_optimized();

        // Only creator or admin can reject
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap_optimized();
        if caller != task.created_by && caller != admin {
            panic!();
        }

        escrow::reject_milestone(env.clone(), task_id, milestone_id, feedback.clone());

        events::emit_milestone_rejected(&env, task_id, milestone_id, feedback);
    }

    /// Approve a milestone and create a time-locked vesting vault for the
    /// payout. The funds remain in escrow for the vesting period instead of
    /// being transferred immediately to the assignee.
    pub fn approve_milestone_with_vesting(
        env: Env,
        caller: Address,
        task_id: u32,
        milestone_id: u32,
        vesting_period: u64,
        feedback: Option<Symbol>,
    ) -> u32 {
        caller.require_auth();

        let task: Task = env
            .storage()
            .instance()
            .get(&DataKey::Task(task_id))
            .unwrap_or_else(|| panic!("task not found"));

        // Only creator or admin can approve
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        if caller != task.created_by && caller != admin {
            panic!("not authorized");
        }

        let amount =
            escrow::approve_milestone(env.clone(), task_id, milestone_id, feedback.clone());

        let token_contract: Address = env
            .storage()
            .instance()
            .get(&DataKey::TokenContract)
            .unwrap();
        let assignee = task.assignee.clone().unwrap();

        // Create vesting vault instead of transferring immediately
        let vault_id = vesting_vault::create_vesting_vault(
            env.clone(),
            task_id,
            milestone_id,
            assignee.clone(),
            amount,
            token_contract,
            vesting_period,
        );

        let vault = vesting_vault::get_vault(&env, vault_id).unwrap();
        events::emit_vesting_vault_created(
            &env,
            vault_id,
            task_id,
            milestone_id,
            assignee.clone(),
            amount,
            vault.vesting_end,
        );

        events::emit_milestone_approved(&env, task_id, milestone_id, amount);

        // Award reputation
        reputation::award_reputation(
            env.clone(),
            assignee,
            reputation::points_for_event(reputation::ReputationEventType::MilestoneApproved),
            reputation::ReputationEventType::MilestoneApproved,
            Some(task_id),
            Symbol::new(&env, "vested"),
        );

        vault_id
    }

    pub fn get_milestones(env: Env, task_id: u32) -> Vec<escrow::Milestone> {
        escrow::get_milestones_for_task(env, task_id)
    }

    // ========================================================================
    // User Profile Management
    // ========================================================================

    pub fn create_profile(env: Env, user: Address, username: Symbol, bio: Symbol) {
        user_profile::create_profile(env.clone(), user.clone(), username.clone(), bio.clone());
        events::emit_profile_created(&env, user, username);
    }

    pub fn update_bio(env: Env, user: Address, new_bio: Symbol) {
        user_profile::update_bio(env.clone(), user.clone(), new_bio.clone());
        events::emit_profile_updated(&env, user, Symbol::new(&env, "bio"));
    }

    pub fn reward_contribution(env: Env, admin: Address, user: Address, points: u32) {
        user_profile::reward_contribution(env.clone(), admin, user.clone(), points);

        let new_total = reputation::get_user_reputation(env.clone(), user.clone());
        events::emit_reputation_awarded(&env, user, points, new_total);
    }

    pub fn get_profile(env: Env, user: Address) -> Option<user_profile::UserProfile> {
        user_profile::get_profile(env, user)
    }

    // ========================================================================
    // Reputation System
    // ========================================================================

    pub fn get_user_reputation(env: Env, user: Address) -> u32 {
        reputation::get_user_reputation(env, user)
    }

    pub fn get_user_tier(env: Env, user: Address) -> Symbol {
        reputation::get_user_tier(env, user)
    }

    pub fn get_leaderboard(env: Env) -> Vec<(Address, u32)> {
        reputation::get_leaderboard(env)
    }

    // ========================================================================
    // Access Control
    // ========================================================================

    pub fn grant_role(env: Env, admin: Address, user: Address, role: access_control::Role) {
        access_control::grant_role(env.clone(), admin.clone(), user.clone(), role.clone());
        events::emit_role_granted(&env, user, format_role(&env, &role), admin);
    }

    pub fn revoke_role(env: Env, admin: Address, user: Address) {
        let role_data = access_control::get_role(env.clone(), user.clone());
        let role_name = role_data
            .as_ref()
            .map(|r| format_role(&env, &r.role))
            .unwrap_or_else(|| Symbol::new(&env, "none"));

        access_control::revoke_role(env.clone(), admin.clone(), user.clone());
        events::emit_role_revoked(&env, user, role_name, admin);
    }

    pub fn has_role(env: Env, user: Address, role: access_control::Role) -> bool {
        access_control::has_role(env, user, role)
    }

    // ========================================================================
    // Governance
    // ========================================================================

    pub fn create_proposal(env: Env, proposer: Address, title: Symbol, description: Symbol) -> u32 {
        let config = governance::get_config(env.clone());

        let proposal_id = governance::create_proposal(
            env.clone(),
            proposer.clone(),
            title.clone(),
            description,
            Some(config.quorum),
            Some(config.threshold),
            config.min_reputation_to_propose,
        );

        events::emit_proposal_created(&env, proposal_id, proposer, title);
        proposal_id
    }

    pub fn cast_vote(env: Env, voter: Address, proposal_id: u32, vote_type: governance::VoteType) {
        let weight = reputation::get_user_reputation(env.clone(), voter.clone());

        governance::cast_vote(env.clone(), voter.clone(), proposal_id, vote_type, weight);

        let vote_str = match vote_type {
            governance::VoteType::For => Symbol::new(&env, "for"),
            governance::VoteType::Against => Symbol::new(&env, "against"),
            governance::VoteType::Abstain => Symbol::new(&env, "abstain"),
        };

        events::emit_vote_cast(&env, proposal_id, voter, vote_str, weight);
    }

    pub fn execute_proposal(env: Env, caller: Address, proposal_id: u32) -> bool {
        let passed = governance::execute_proposal(env.clone(), caller, proposal_id);
        events::emit_proposal_executed(&env, proposal_id, passed);
        passed
    }

    pub fn get_proposal(env: Env, proposal_id: u32) -> Option<governance::Proposal> {
        governance::get_proposal(env, proposal_id)
    }

    pub fn get_active_proposals(env: Env) -> Vec<governance::Proposal> {
        governance::get_active_proposals(env)
    }

    // ========================================================================
    // Admin Multisig
    // ========================================================================
    //
    // Note: these endpoints are prefixed `multisig_*` rather than taking the
    // bare `create_proposal` / `execute_proposal` names, which are already
    // exported above by the reputation-weighted governance module. The
    // approval-vote entry point keeps the unprefixed `vote_proposal` name,
    // which was free.

    pub fn configure_multisig(
        env: Env,
        admin: Address,
        signers: Vec<Address>,
        threshold: u32,
        proposal_ttl: Option<u64>,
        auto_execute: Option<bool>,
    ) {
        let signer_count = signers.len();
        multisig::configure(
            env.clone(),
            admin.clone(),
            signers,
            threshold,
            proposal_ttl,
            auto_execute,
        );
        events::emit_multisig_configured(&env, admin, signer_count, threshold);
    }

    pub fn multisig_propose(
        env: Env,
        proposer: Address,
        description: Symbol,
        action: multisig::MultisigAction,
    ) -> u32 {
        let proposal_id =
            multisig::propose(env.clone(), proposer.clone(), description.clone(), action);

        let threshold = multisig::get_config(&env).threshold;
        events::emit_multisig_proposed(&env, proposal_id, proposer, description, threshold);

        proposal_id
    }

    pub fn vote_proposal(
        env: Env,
        signer: Address,
        proposal_id: u32,
    ) -> multisig::MultisigProposalStatus {
        let status = multisig::vote_proposal(env.clone(), signer.clone(), proposal_id);

        let approvals = multisig::get_approval_count(&env, proposal_id);
        let threshold = multisig::get_config(&env).threshold;
        events::emit_multisig_approved(&env, proposal_id, signer.clone(), approvals, threshold);

        if status == multisig::MultisigProposalStatus::Executed {
            events::emit_multisig_executed(&env, proposal_id, signer);
        }

        status
    }

    pub fn multisig_execute_proposal(
        env: Env,
        caller: Address,
        proposal_id: u32,
    ) -> multisig::MultisigProposalStatus {
        let status = multisig::execute_proposal(env.clone(), caller.clone(), proposal_id);
        events::emit_multisig_executed(&env, proposal_id, caller);
        status
    }

    pub fn multisig_cancel_proposal(env: Env, caller: Address, proposal_id: u32) {
        multisig::cancel_proposal(env.clone(), caller.clone(), proposal_id);
        events::emit_multisig_cancelled(&env, proposal_id, caller);
    }

    pub fn get_multisig_proposal(env: Env, proposal_id: u32) -> Option<multisig::MultisigProposal> {
        multisig::get_proposal(&env, proposal_id)
    }

    pub fn get_multisig_config(env: Env) -> multisig::MultisigConfig {
        multisig::get_config(&env)
    }

    pub fn get_multisig_approval_count(env: Env, proposal_id: u32) -> u32 {
        multisig::get_approval_count(&env, proposal_id)
    }

    pub fn has_approved_proposal(env: Env, proposal_id: u32, signer: Address) -> bool {
        multisig::has_approved(&env, proposal_id, &signer)
    }

    pub fn get_pending_multisig_proposals(env: Env) -> Vec<multisig::MultisigProposal> {
        multisig::get_pending_proposals(&env)
    }

    pub fn is_multisig_signer(env: Env, who: Address) -> bool {
        multisig::is_signer(&env, &who)
    }

    // ========================================================================
    // Pause Control
    // ========================================================================

    pub fn pause(env: Env, admin: Address, action: pausable::PauseAction) {
        pausable::pause(env.clone(), admin.clone(), action);
        events::emit_paused(&env, format_pause_action(&env, action), admin);
    }

    pub fn unpause(env: Env, admin: Address, action: pausable::PauseAction) {
        pausable::unpause(env.clone(), admin.clone(), action);
        events::emit_unpaused(&env, format_pause_action(&env, action), admin);
    }

    pub fn pause_all(env: Env, admin: Address) {
        pausable::pause_all(env.clone(), admin.clone());
        events::emit_paused(&env, Symbol::new(&env, "all"), admin);
    }

    pub fn unpause_all(env: Env, admin: Address) {
        pausable::unpause_all(env.clone(), admin.clone());
        events::emit_unpaused(&env, Symbol::new(&env, "all"), admin);
    }

    // ========================================================================
    // Multi-Asset Swap Router
    // ========================================================================

    pub fn configure_swap_router(
        env: Env,
        admin: Address,
        oracle: Address,
        max_hops: u32,
        default_slippage_bps: u32,
    ) {
        swap_router::configure(
            env.clone(),
            admin.clone(),
            oracle.clone(),
            max_hops,
            default_slippage_bps,
        );
        events::emit_router_configured(&env, admin, oracle, max_hops, default_slippage_bps);
    }

    pub fn add_approved_stablecoin(env: Env, admin: Address, stablecoin: Address) {
        swap_router::add_approved_stablecoin(env.clone(), admin.clone(), stablecoin.clone());
        events::emit_stablecoin_approved(&env, admin, stablecoin);
    }

    pub fn remove_approved_stablecoin(env: Env, admin: Address, stablecoin: Address) {
        swap_router::remove_approved_stablecoin(env.clone(), admin.clone(), stablecoin.clone());
        events::emit_stablecoin_removed(&env, admin, stablecoin);
    }

    pub fn get_approved_stablecoins(env: Env) -> Vec<Address> {
        swap_router::get_approved_stablecoins(env)
    }

    pub fn get_swap_router_config(env: Env) -> swap_router::RouterConfig {
        swap_router::get_config(env)
    }

    pub fn convert_incoming_deposit(
        env: Env,
        sender: Address,
        token_in: Address,
        amount_in: i128,
        route: swap_router::SwapRoute,
        slippage_bps: Option<u32>,
    ) -> swap_router::ConversionOutcome {
        let outcome = swap_router::convert_incoming_deposit(
            env.clone(),
            sender.clone(),
            token_in.clone(),
            amount_in,
            route,
            slippage_bps,
        );

        match &outcome {
            swap_router::ConversionOutcome::Converted(token_out, amount_out) => {
                events::emit_swap_executed(
                    &env,
                    sender,
                    token_in,
                    token_out.clone(),
                    amount_in,
                    *amount_out,
                );
            }
            swap_router::ConversionOutcome::Refunded(reason) => {
                events::emit_swap_refunded(&env, sender, token_in, amount_in, reason.clone());
            }
        }

        outcome
    }

    pub fn get_vault_balance(env: Env, owner: Address, stablecoin: Address) -> i128 {
        swap_router::get_vault_balance(env, owner, stablecoin)
    }

    pub fn withdraw_stablecoin(env: Env, owner: Address, stablecoin: Address, amount: i128) {
        swap_router::withdraw_stablecoin(env, owner, stablecoin, amount);
    }

    pub fn get_swap_router_stats(env: Env) -> swap_router::SwapRouterStats {
        swap_router::get_stats(env)
    }

    // ========================================================================
    // Statistics & Views
    // ========================================================================

    pub fn get_task(env: Env, task_id: u32) -> Option<Task> {
        env.storage().instance().get(&DataKey::Task(task_id))
    }

    pub fn get_escrow_stats(env: Env) -> escrow::EscrowStats {
        escrow::get_escrow_stats(env)
    }

    pub fn get_statistics(env: Env) -> storage::ContractStatistics {
        storage::get_statistics(&env)
    }

    // ========================================================================
    // Multi-Stablecoin Vault
    // ========================================================================

    pub fn add_supported_token(env: Env, admin: Address, token: Address) {
        admin.require_auth();

        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_optimized();
        if admin != stored_admin {
            panic!();
        }

        vault::add_supported_token(&env, token.clone());
        events::emit_token_supported(&env, token, admin);
    }

    pub fn remove_supported_token(env: Env, admin: Address, token: Address) {
        admin.require_auth();

        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_optimized();
        if admin != stored_admin {
            panic!();
        }

        vault::remove_supported_token(&env, token.clone());
        events::emit_token_unsupported(&env, token, admin);
    }

    pub fn is_token_supported(env: Env, token: Address) -> bool {
        vault::is_supported_token(&env, &token)
    }

    pub fn get_supported_tokens(env: Env) -> Vec<Address> {
        vault::get_supported_tokens(&env)
    }

    pub fn deposit_to_vault(env: Env, depositor: Address, token: Address, amount: i128) {
        depositor.require_auth();
        vault::deposit(&env, depositor.clone(), token.clone(), amount);
        events::emit_vault_deposit(&env, depositor, token, amount);
    }

    pub fn claim_from_vault(env: Env, claimant: Address, token: Address, amount: i128) {
        claimant.require_auth();
        vault::claim(&env, claimant.clone(), token.clone(), amount);
        events::emit_vault_claim(&env, claimant, token, amount);
    }

    pub fn get_token_vault_balance(env: Env, token: Address) -> i128 {
        vault::get_vault_balance(&env, token)
    }

    pub fn get_depositor_vault_balance(env: Env, depositor: Address, token: Address) -> i128 {
        vault::get_depositor_balance(&env, depositor, token)
    }

    // ========================================================================
    // Merkle Payroll (Vault)
    // ========================================================================

    pub fn set_payroll_root(
        env: Env,
        admin: Address,
        payroll_id: u32,
        root: soroban_sdk::BytesN<32>,
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

        vault::set_payroll_root(&env, payroll_id, root);
    }

    pub fn claim_payroll(
        env: Env,
        claimant: Address,
        token: Address,
        payroll_id: u32,
        amount: i128,
        proof: Vec<soroban_sdk::BytesN<32>>,
    ) {
        claimant.require_auth();
        vault::claim_payroll(&env, claimant, token, payroll_id, amount, proof);
    }

    // ========================================================================
    // Vesting Vaults (Time-Locked Milestone Payouts)
    // ========================================================================

    /// Create a time-locked vesting vault for an approved milestone payout.
    /// The funds are held for a safety window during which disputes can be
    /// opened. After the vesting period, the beneficiary can claim the funds
    /// if no dispute is active.
    pub fn create_vesting_vault(
        env: Env,
        caller: Address,
        task_id: u32,
        milestone_id: u32,
        beneficiary: Address,
        amount: i128,
        token: Address,
        vesting_period: u64,
    ) -> u32 {
        caller.require_auth();

        // Verify caller is task creator or admin
        let task: Task = env
            .storage()
            .instance()
            .get(&DataKey::Task(task_id))
            .unwrap_or_else(|| panic!("task not found"));

        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        if caller != task.created_by && caller != admin {
            panic!("not authorized");
        }

        let vault_id = vesting_vault::create_vesting_vault(
            env.clone(),
            task_id,
            milestone_id,
            beneficiary.clone(),
            amount,
            token,
            vesting_period,
        );

        let vault = vesting_vault::get_vault(&env, vault_id).unwrap();
        events::emit_vesting_vault_created(
            &env,
            vault_id,
            task_id,
            milestone_id,
            beneficiary,
            amount,
            vault.vesting_end,
        );

        vault_id
    }

    /// Open a dispute on an active vesting vault. Only admin or task creator
    /// can dispute during the vesting period.
    pub fn dispute_vesting_vault(
        env: Env,
        caller: Address,
        vault_id: u32,
        reason: String,
    ) {
        vesting_vault::dispute_vesting_vault(env.clone(), caller.clone(), vault_id, reason.clone());
        events::emit_vesting_vault_disputed(&env, vault_id, caller, reason);
    }

    /// Release vested funds to the beneficiary after the vesting period ends.
    /// Only callable if no dispute is active.
    pub fn release_vesting_vault(env: Env, caller: Address, vault_id: u32) -> i128 {
        let vault = vesting_vault::get_vault(&env, vault_id).unwrap();
        let beneficiary = vault.beneficiary.clone();
        let amount = vesting_vault::release_vesting_vault(env.clone(), caller, vault_id);
        events::emit_vesting_vault_released(&env, vault_id, beneficiary, amount);
        amount
    }

    /// Refund disputed vault funds back to the task creator. Only admin can
    /// refund a disputed vault.
    pub fn refund_vesting_vault(env: Env, admin: Address, vault_id: u32) -> i128 {
        let vault = vesting_vault::get_vault(&env, vault_id).unwrap();
        let task: Task = env
            .storage()
            .instance()
            .get(&DataKey::Task(vault.task_id))
            .unwrap();
        let amount = vesting_vault::refund_vesting_vault(env.clone(), admin, vault_id);
        events::emit_vesting_vault_refunded(&env, vault_id, task.created_by, amount);
        amount
    }

    /// Get vesting vault details.
    pub fn get_vesting_vault(
        env: Env,
        vault_id: u32,
    ) -> Option<vesting_vault::VestingVault> {
        vesting_vault::get_vault(&env, vault_id)
    }

    /// Get all vault IDs for a task.
    pub fn get_task_vesting_vaults(env: Env, task_id: u32) -> Vec<u32> {
        vesting_vault::get_task_vaults(&env, task_id)
    }

    /// Get all vault IDs for a beneficiary.
    pub fn get_beneficiary_vesting_vaults(env: Env, beneficiary: Address) -> Vec<u32> {
        vesting_vault::get_beneficiary_vaults(&env, &beneficiary)
    }

    /// Check if a vault's vesting period is complete.
    pub fn is_vesting_complete(env: Env, vault_id: u32) -> bool {
        vesting_vault::is_vesting_complete(&env, vault_id)
    }

    /// Get remaining vesting time in seconds. Returns 0 if vesting is complete
    /// or vault is not active.
    pub fn get_remaining_vesting_time(env: Env, vault_id: u32) -> u64 {
        vesting_vault::get_remaining_vesting_time(&env, vault_id)
    }

    // ========================================================================
    // Upgrade Timelock & Rollback Guard
    // ========================================================================

    pub fn get_upgrade_timelock(env: Env) -> u64 {
        upgrade::get_timelock_seconds(&env)
    }

    pub fn set_upgrade_timelock(env: Env, admin: Address, seconds: u64) {
        let old_seconds = upgrade::get_timelock_seconds(&env);
        let new_seconds = upgrade::set_timelock_seconds(env.clone(), admin.clone(), seconds);
        events::emit_upgrade_timelock_updated(&env, old_seconds, new_seconds, admin);
    }

    pub fn propose_upgrade(
        env: Env,
        admin: Address,
        new_wasm_hash: soroban_sdk::BytesN<32>,
    ) -> upgrade::UpgradeProposal {
        let proposal = upgrade::propose_upgrade(env.clone(), admin.clone(), new_wasm_hash.clone());
        events::emit_upgrade_proposed(&env, new_wasm_hash, admin, proposal.ready_at);
        proposal
    }

    pub fn veto_upgrade(env: Env, guardian: Address) -> upgrade::UpgradeProposal {
        let proposal = upgrade::veto_upgrade(env.clone(), guardian.clone());
        events::emit_upgrade_vetoed(&env, proposal.wasm_hash.clone(), guardian);
        proposal
    }

    pub fn execute_upgrade(env: Env, admin: Address) -> soroban_sdk::BytesN<32> {
        let wasm_hash = upgrade::execute_upgrade(env.clone(), admin.clone());
        events::emit_upgrade_executed(&env, wasm_hash.clone(), admin);
        wasm_hash
    }

    pub fn get_pending_upgrade(env: Env) -> Option<upgrade::UpgradeProposal> {
        upgrade::get_pending_upgrade(&env)
    }

    pub fn get_upgrade_history(env: Env) -> Vec<upgrade::UpgradeHistoryEntry> {
        upgrade::get_upgrade_history(&env)
    }

    // ========================================================================
    // Reward Treasury: Decay-Curve Vesting & Distribution
    // ========================================================================

    pub fn configure_treasury(env: Env, admin: Address, token: Address) {
        treasury::configure_treasury(env, admin, token);
    }

    pub fn fund_treasury(env: Env, funder: Address, amount: i128) -> i128 {
        let new_balance = treasury::fund_treasury(env.clone(), funder.clone(), amount);
        events::emit_treasury_funded(&env, funder, amount, new_balance);
        new_balance
    }

    pub fn create_vesting_schedule(
        env: Env,
        admin: Address,
        beneficiary: Address,
        total_amount: i128,
        start_time: u64,
        cliff_seconds: u64,
        period_seconds: u64,
        decay_rate_bps: u32,
    ) -> u32 {
        let schedule_id = treasury::create_vesting_schedule(
            env.clone(),
            admin,
            beneficiary.clone(),
            total_amount,
            start_time,
            cliff_seconds,
            period_seconds,
            decay_rate_bps,
        );
        events::emit_vesting_schedule_created(
            &env,
            schedule_id,
            beneficiary,
            total_amount,
            decay_rate_bps,
        );
        schedule_id
    }

    pub fn claim_vesting(env: Env, beneficiary: Address, schedule_id: u32) -> i128 {
        let amount = treasury::claim(env.clone(), beneficiary.clone(), schedule_id);
        let schedule = treasury::get_vesting_schedule(&env, schedule_id).unwrap_optimized();
        events::emit_vesting_claimed(
            &env,
            schedule_id,
            beneficiary,
            amount,
            schedule.claimed_amount,
        );
        amount
    }

    pub fn get_vesting_schedule(env: Env, schedule_id: u32) -> Option<treasury::VestingSchedule> {
        treasury::get_vesting_schedule(&env, schedule_id)
    }

    pub fn get_beneficiary_schedules(env: Env, beneficiary: Address) -> Vec<u32> {
        treasury::get_beneficiary_schedules(env, beneficiary)
    }

    pub fn get_vested_amount(env: Env, schedule_id: u32) -> i128 {
        treasury::vested_amount(&env, schedule_id)
    }

    pub fn get_claimable_amount(env: Env, schedule_id: u32) -> i128 {
        treasury::claimable_amount(&env, schedule_id)
    }

    pub fn get_treasury_balance(env: Env) -> i128 {
        treasury::get_treasury_balance(&env)
    }

    pub fn get_treasury_allocated(env: Env) -> i128 {
        treasury::get_allocated_total(&env)
    }
}

// ============================================================================
// Dispute Split Resolution (module-level for multisig reuse)
// ============================================================================

pub fn resolve_dispute_split(
    env: Env,
    task_id: u32,
    recipients: Vec<Address>,
    shares_bps: Vec<u32>,
) {
    // ── Basic invariants ────────────────────────────────────────────
    if recipients.len() == 0 {
        panic!();
    }
    if recipients.len() != shares_bps.len() {
        panic!();
    }

    let mut total_bps: u32 = 0;
    for i in 0..shares_bps.len() {
        let bps = shares_bps.get(i).unwrap_optimized();
        total_bps = total_bps
            .checked_add(bps)
            .unwrap_optimized();
    }
    if total_bps != 10000 {
        panic!();
    }

    // ── Task state checks ──────────────────────────────────────────
    let mut task: Task = env
        .storage()
        .instance()
        .get(&DataKey::Task(task_id))
        .unwrap_optimized();

    if task.status != TaskStatus::Disputed {
        panic!();
    }

    // ── Fee calculation ────────────────────────────────────────────
    let platform_fee_bps: u32 = env
        .storage()
        .instance()
        .get(&DataKey::PlatformFeeBps)
        .unwrap_or(0);
    let fee_recipient: Address = env
        .storage()
        .instance()
        .get(&DataKey::FeeRecipient)
        .unwrap_optimized();
    let token_contract: Address = env
        .storage()
        .instance()
        .get(&DataKey::TokenContract)
        .unwrap_optimized();
    let token_client = soroban_sdk::token::Client::new(&env, &token_contract);

    let fee = (task.reward * platform_fee_bps as i128) / 10000;
    let distributable = task.reward - fee;

    // ── Distribute ─────────────────────────────────────────────────
    for i in 0..recipients.len() {
        let recipient = recipients.get(i).unwrap_optimized();
        let bps = shares_bps.get(i).unwrap_optimized();
        let payout = (distributable * bps as i128) / 10000;
        if payout > 0 {
            token_client.transfer(
                &env.current_contract_address(),
                &recipient,
                &payout,
            );
        }
    }

    // Send the platform fee
    if fee > 0 {
        token_client.transfer(
            &env.current_contract_address(),
            &fee_recipient,
            &fee,
        );
    }

    // ── Release escrow ─────────────────────────────────────────────
    escrow::release_escrow(env.clone(), task_id, task.reward);

    task.status = TaskStatus::Resolved;
    task.updated_at = env.ledger().timestamp();
    env.storage()
        .instance()
        .set(&DataKey::Task(task_id), &task);

    events::emit_dispute_split_resolved(&env, task_id, fee, distributable);
}

// ============================================================================
// Helper Functions
// ============================================================================

fn format_role(env: &Env, role: &access_control::Role) -> Symbol {
    match role {
        access_control::Role::Admin => Symbol::new(&env, "Admin"),
        access_control::Role::Manager => Symbol::new(&env, "Manager"),
        access_control::Role::Moderator => Symbol::new(&env, "Moderator"),
        access_control::Role::Verifier => Symbol::new(&env, "Verifier"),
        access_control::Role::Guardian => Symbol::new(&env, "Guardian"),
    }
}

fn format_pause_action(env: &Env, action: pausable::PauseAction) -> Symbol {
    match action {
        pausable::PauseAction::CreateTask => Symbol::new(&env, "create_task"),
        pausable::PauseAction::AssignTask => Symbol::new(&env, "assign_task"),
        pausable::PauseAction::SubmitWork => Symbol::new(&env, "submit_work"),
        pausable::PauseAction::CompleteTask => Symbol::new(&env, "complete_task"),
        pausable::PauseAction::CancelTask => Symbol::new(&env, "cancel_task"),
        pausable::PauseAction::DisputeTask => Symbol::new(&env, "dispute_task"),
        pausable::PauseAction::Withdraw => Symbol::new(&env, "withdraw"),
        pausable::PauseAction::All => Symbol::new(&env, "all"),
    }
}
