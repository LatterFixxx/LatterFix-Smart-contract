# LatterFix Smart Contract

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/Rust-1.84%2B-orange.svg)](https://www.rust-lang.org)
[![Stellar](https://img.shields.io/badge/Stellar-Soroban%2021-blue.svg)](https://stellar.org)
[![Tests](https://img.shields.io/badge/Tests-Passing-brightgreen.svg)](https://github.com/LatterFixxx/LatterFix-Smart-contract)

<p align="center">
  <img src="assets/latterfix_hero_banner.png" alt="LatterFix Hero Banner" width="100%" />
</p>

<div align="center">
  <h3>TaskManager Pro — Soroban Smart Contract</h3>
  <p><i>Enterprise-grade decentralized task management protocol with escrow, governance, and reputation system on the Stellar Network.</i></p>
</div>

---

## What is LatterFix?

**LatterFix** (TaskManager Pro) is a decentralized platform designed to redefine how organizations and independent builders collaborate. By replacing trust-based arrangements with automated, secure, and verifiable smart contract workflows, LatterFix ensures that contributors get paid for their verified contributions while creators get the quality deliverables they expect.

### Core Pillars:
1. **Decentralized Escrows & Milestones**: Funds are locked in a secure smart contract when tasks are created. Payments can be released incrementally through predefined milestones or fully upon total task verification.
2. **On-Chain Reputation System**: Every contributor has a dynamically adjusted reputation score. Reputation grows with successful task completions and milestone approvals, and drops with cancellations or lost disputes.
3. **Decentralized Governance**: Platform parameters and configurations are governed by the community. Proposal weight is tied directly to users' on-chain reputation scores, encouraging positive participation.
4. **Built-in Dispute Resolution**: If disputes arise during the delivery cycle, platform administrators or delegated moderators can mediate and split the escrowed funds fairly based on the evidence provided.

---

## Overview

This repository contains the Soroban smart contract powering the LatterFix TaskManager Pro platform. The contract implements a complete decentralized escrow workflow — locking task rewards on-chain, routing platform fees, milestone-based payments, dispute resolution, governance, and a comprehensive reputation system.

---

## Contract Architecture

### Module Structure

| Module | File | Description |
|--------|------|-------------|
| **Core** | `lib.rs` | Main contract with task lifecycle, escrow integration, and entry points |
| **Access Control** | `access_control.rs` | Role-based access control (RBAC) system |
| **Escrow** | `escrow.rs` | Milestone-based payment escrow management |
| **Events** | `events.rs` | Standardized event emission for off-chain indexing |
| **Gasless** | `gasless.rs` | Ed25519 signature-based gasless task assignment via a relayer |
| **Governance** | `governance.rs` | Proposal and voting system for protocol decisions |
| **Pausable** | `pausable.rs` | Emergency pause functionality per action type |
| **Reputation** | `reputation.rs` | User reputation and tier system |
| **Social Recovery** | `social_recovery.rs` | Threshold admin-role recovery by Master/Legend guardians |
| **Storage** | `storage.rs` | Storage optimization and TTL management utilities |
| **User Profile** | `user_profile.rs` | User profiles with reputation tracking |

---

## Key Features

### 1. Task Management
- Create tasks with escrowed rewards
- Milestone-based task breakdown
- Task assignment and submission workflow
- Verification and completion process
- Cancellation with automatic refunds
- Dispute resolution by admin

### 2. Escrow System
- Lock tokens on task creation
- Milestone-based payments
- Partial releases per approved milestone
- Statistics tracking (locked, released, refunded)

### 3. Reputation System
- Points-based reputation (starting: 100)
- Tier levels: Newcomer → Contributor → Expert → Master → Legend
- Points awarded for completed tasks, verified work, milestones
- Penalties for cancellations and lost disputes
- Global leaderboard

### 4. Governance
- Create proposals (requires minimum reputation)
- Vote on proposals (weight based on reputation)
- Quorum and threshold requirements
- Execute passed proposals

### 5. Access Control
- Role-based permissions (Admin, Manager, Moderator, Verifier)
- Grant/revoke roles
- Permission-gated operations

### 6. Pausable
- Pause/unpause specific actions
- Emergency circuit breaker
- Admin-only control

### 7. Social Recovery
- Emergency admin-role rotation when the admin key is lost or compromised
- Threshold approval by Master/Legend tier guardians (default 3)
- State machine (Pending → Approved → Executed) with TTL expiry and admin veto
- Approvals re-validated against live tiers and admin at execution time

---

## Contract Methods

### Initialization

```rust
fn initialize(
    env: Env,
    admin: Address,
    platform_fee_bps: u32,    // Platform fee in basis points (max 10%)
    token_contract: Address,   // Stellar token contract address
    fee_recipient: Address,    // Address to receive platform fees
)
```

### Task Operations

| Method | Description |
|--------|-------------|
| `create_task(creator, title, description, reward, tags)` | Create a new task with escrowed reward |
| `create_task_with_milestones(creator, title, description, milestones, tags)` | Create task with milestone breakdown |
| `assign_task(assignee, task_id)` | Claim an open task |
| `submit_work(assignee, task_id, delivery_url)` | Submit completed work |
| `complete_task(caller, task_id)` | Approve and release payment |
| `cancel_task(creator, task_id)` | Cancel and refund |
| `dispute_task(caller, task_id)` | Raise a dispute |
| `resolve_dispute(admin, task_id, creator_refund, assignee_payout)` | Admin resolves dispute |
| `get_task(task_id)` | Get task details |

### Milestone Operations

| Method | Description |
|--------|-------------|
| `submit_milestone(assignee, task_id, milestone_id, submission_url)` | Submit milestone work |
| `approve_milestone(caller, task_id, milestone_id, feedback)` | Approve and release milestone payment |
| `reject_milestone(caller, task_id, milestone_id, feedback)` | Reject milestone |
| `get_milestones(task_id)` | Get all milestones for a task |

### Profile Operations

| Method | Description |
|--------|-------------|
| `create_profile(user, username, bio)` | Create user profile |
| `update_bio(user, new_bio)` | Update biography |
| `get_profile(user)` | Get profile data |

### Reputation Operations

| Method | Description |
|--------|-------------|
| `get_user_reputation(user)` | Get reputation points |
| `get_user_tier(user)` | Get current tier |
| `get_leaderboard()` | Get top users by reputation |

### Governance Operations

| Method | Description |
|--------|-------------|
| `create_proposal(proposer, title, description)` | Create a new proposal |
| `cast_vote(voter, proposal_id, vote_type)` | Vote on a proposal |
| `execute_proposal(caller, proposal_id)` | Execute a passed proposal |
| `get_proposal(proposal_id)` | Get proposal details |
| `get_active_proposals()` | Get all active proposals |

### Access Control

| Method | Description |
|--------|-------------|
| `grant_role(admin, user, role)` | Grant a role to user |
| `revoke_role(admin, user)` | Revoke user's role |
| `has_role(user, role)` | Check if user has role |

### Pause Control

| Method | Description |
|--------|-------------|
| `pause(admin, action)` | Pause specific action |
| `unpause(admin, action)` | Unpause specific action |
| `pause_all(admin)` | Pause all actions |
| `unpause_all(admin)` | Unpause all actions |

---

## Data Structures

### Task

```rust
pub struct Task {
    pub id: u32,
    pub title: String,
    pub description: String,
    pub reward: i128,
    pub assignee: Option<Address>,
    pub status: TaskStatus,
    pub created_by: Address,
    pub tags: Vec<String>,
    pub category_id: Option<u32>,
    pub deadline: Option<u64>,
    pub created_at: u64,
    pub updated_at: u64,
}
```

### Task Status Lifecycle

```
Open → InProgress → Completed → Verified
  │         │            │
  └─────────┴────────────┴──────── Cancelled / Disputed → Resolved
```

### Milestone

```rust
pub struct Milestone {
    pub id: u32,
    pub task_id: u32,
    pub title: String,
    pub amount: i128,
    pub status: MilestoneStatus,
    pub due_date: Option<u64>,
    pub submission_url: Option<String>,
    pub feedback: Option<String>,
}
```

### User Profile

```rust
pub struct UserProfile {
    pub address: Address,
    pub username: String,
    pub reputation: u32,
    pub completed_tasks: u32,
    pub joined_at: u64,
    pub bio: String,
}
```

### Reputation Tiers

| Tier | Points Range |
|------|--------------|
| Newcomer | 0 - 99 |
| Contributor | 100 - 499 |
| Expert | 500 - 999 |
| Master | 1000 - 2499 |
| Legend | 2500+ |

---

## Events

The contract emits standardized events for off-chain indexing:

- `task_created` - New task created
- `task_assigned` - Task claimed by assignee
- `task_submitted` - Work submitted
- `task_completed` - Task verified and paid
- `task_cancelled` - Task cancelled
- `task_disputed` - Dispute raised
- `milestone_*` - Milestone lifecycle events
- `prop_created` - Governance proposal created
- `vote_cast` - Vote recorded
- `role_grant` / `role_revoke` - Role changes
- `paused` / `unpaused` - Pause state changes

---

## Development

### Prerequisites

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Add WebAssembly target
rustup target add wasm32-unknown-unknown

# Install Stellar CLI
cargo install --locked stellar-cli
```

### Run Tests

```bash
cargo test
```

### Build WASM Binary

```bash
cargo build --target wasm32-unknown-unknown --release
```

### Build Optimized WASM

```bash
stellar contract build
```

### Deploy to Testnet

```bash
# Configure testnet
stellar network add testnet \
  --rpc-url https://soroban-testnet.stellar.org:443 \
  --network-passphrase "Test SDF Network ; September 2015"

# Deploy
stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/task_manager_pro.wasm \
  --network testnet \
  --source <your-account>
```

### Running the TTL Monitoring Bot

The repository includes an automated background bot (`tooling/ttl_bot.py`) to track contract storage TTL and send bump transactions before expiration.

**1. Install Dependencies**
```bash
pip install stellar-sdk>=9.0.0 requests
```

**2. Run the Bot**
```bash
python3 tooling/ttl_bot.py \
  --secret-key "<YOUR_SECRET_KEY>" \
  --contract-ids "<CONTRACT_ID_1>,<CONTRACT_ID_2>" \
  --interval 3600
```

**Bot Options:**
- `--dry-run`: Run without actually sending transactions.
- `--threshold`: TTL threshold in ledgers to trigger renewal (default: `10000`).
- `--extend-to`: Number of ledgers to extend the TTL to (default: `50000`).
- `--min-balance`: Minimum XLM balance for the bot wallet before alerting (default: `10.0`).
- `--interval`: Run continuously every N seconds (default: `0`, which means run once and exit).

### Persistent application-state TTL

Persistent keys managed by `src/storage.rs` use a bounded renewal policy:
`TTL_EXTENSION_THRESHOLD` is the minimum remaining lifetime that triggers a
renewal, `DEFAULT_PERSISTENT_TTL` is the normal renewal target, and
`MAX_PERSISTENT_TTL` is the documented ceiling.

Successful reads through `storage::get_persistent` renew an active key when it
falls below the threshold; writes already renew automatically. Maintenance bots
may also call the admin-authenticated `refresh_persistent_storage(admin)`
contract method. That sweep is safe when optional global keys have not been
created yet and is a no-op for entries whose TTL is still healthy.

The contract-level refresh is intentionally separate from `soroban contract
extend`: the CLI command renews contract code/instance TTL, while
`refresh_persistent_storage` renews the persistent application-data entries.

---

## Security Considerations

1. **Re-entrancy**: All state changes happen before external calls
2. **Access Control**: Admin-only functions are protected by `require_auth`
3. **Overflow Protection**: All arithmetic uses checked operations
4. **Pause Mechanism**: Emergency stop capability for critical vulnerabilities
5. **Fee Limits**: Platform fee capped at 10% (1000 basis points)

---

## Gas Optimization

- Storage keys use minimal byte representation
- TTL management prevents storage churn
- Instance storage for frequently accessed data
- Persistent storage for user profiles and historical data

---

## License

MIT License - See [LICENSE](LICENSE) file for details.

---

## Contributing

1. Fork the repository
2. Create a feature branch
3. Run tests: `cargo test`
4. Submit a pull request

---

## Support

- **Issues**: [GitHub Issues](https://github.com/LatterFixxx/LatterFix-Smart-contract/issues)
- **Discord**: [LatterFix Community](https://discord.gg/latterfix)

---

**Built with ❤️ on Stellar/Soroban**
