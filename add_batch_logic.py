import re

with open("src/lib.rs", "r") as f:
    content = f.read()

# 1. Add BatchTask and BatchTaskLeaf structs
structs = """
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
"""
content = re.sub(
    r"(pub struct TaskWithMilestones \{.*?\n\})",
    r"\1\n" + structs,
    content,
    flags=re.DOTALL
)

# 2. Add DataKeys
keys = """    Task(u32),
    TaskCount,
    BatchTask(u32),
    BatchTaskCount,
    BatchTaskClaimed(u32, u32),"""
content = re.sub(
    r"    Task\(u32\),\n    TaskCount,",
    keys,
    content
)

# 3. Add functions
funcs = """
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
"""

# Insert before "    pub fn create_task("
content = content.replace("    pub fn create_task(", funcs + "\n    pub fn create_task(")

with open("src/lib.rs", "w") as f:
    f.write(content)

