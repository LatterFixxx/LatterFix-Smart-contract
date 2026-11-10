# Soroban Diagnostic Event Filter & Subgraph Indexing Specifications
/* Authorized Protocol Quality Assurance & Formal Invariant Verification */

## Overview
This specification document outlines the event topology, topic structures, entity definitions, and GraphQL schema mappings for external indexing pipelines (The Graph, Goldsky, Mercury, and custom indexers) monitoring the `task_manager_pro` contract.

## 1. Diagnostic Event Schema Dictionary

### 1.1 State Transition Snapshot (`diag_snp`)
Emitted on major state machine transitions (creation, assignment, completion, cancellation, dispute, recovery).

- **Topic 0**: `Symbol("diag_snp")` (Diagnostic Snapshot)
- **Topic 1**: `entity_id` (`u32`) - Unique identifier of the subject entity (task, recovery proposal, milestone).
- **Data Payload**: Tuple of 5 fields:
  1. `correlation_id` (`BytesN<32>`): Unique client-supplied or contract-derived UUID for transaction tracking across services.
  2. `previous_state` (`Symbol`): Previous lifecycle state (e.g., `Open`, `Assigned`, `Disputed`).
  3. `new_state` (`Symbol`): Resulting lifecycle state after execution (e.g., `Completed`, `Resolved`).
  4. `sequence` (`u32`): Stellar ledger sequence number when the transition was committed.
  5. `timestamp` (`u64`): UNIX timestamp in seconds.

### 1.2 Correlated Operation (`diag_cor`)
Emitted for auditing authorized actor operations with correlation IDs.

- **Topic 0**: `Symbol("diag_cor")` (Correlated Operation)
- **Topic 1**: `entity_id` (`u32`) - Subject identifier.
- **Data Payload**:
  1. `correlation_id` (`BytesN<32>`)
  2. `operation_tag` (`Symbol`): Operation identifier (e.g., `submit_work`, `vote`, `claim`).
  3. `actor` (`Address`): Signer or authorized caller account.
  4. `sequence` (`u32`): Ledger sequence.
  5. `timestamp` (`u64`): Timestamp.

---

## 2. GraphQL Subgraph Schema Definitions

```graphql
"""
Correlated entity state transition captured on-chain
"""
type StateTransitionSnapshot @entity {
  id: ID! # <entity_id>-<sequence>-<timestamp>
  entityId: BigInt!
  correlationId: String!
  previousState: String!
  newState: String!
  ledgerSequence: BigInt!
  timestamp: BigInt!
}

"""
Correlated actor operation audit event
"""
type CorrelatedOperation @entity {
  id: ID! # <correlationId>-<entityId>
  entityId: BigInt!
  correlationId: String!
  operationTag: String!
  actor: String!
  ledgerSequence: BigInt!
  timestamp: BigInt!
}

"""
Contract Task lifecycle entity
"""
type TaskEntity @entity {
  id: ID! # task_id
  creator: String!
  assignee: String
  reward: BigInt!
  status: String!
  lastCorrelationId: String
  transitions: [StateTransitionSnapshot!]! @derivedFrom(field: "entityId")
}
```

---

## 3. Subgraph Indexer Mapping Implementation Guide

When configuring `subgraph.yaml` for Soroban / Stellar networks:

```yaml
specVersion: 0.0.5
schema:
  file: ./schema.graphql
dataSources:
  - kind: stellar
    name: TaskManagerContract
    network: testnet
    source:
      contractId: "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM"
    mapping:
      kind: stellar/subgraph
      apiVersion: 0.0.7
      language: wasm/assemblyscript
      entities:
        - StateTransitionSnapshot
        - CorrelatedOperation
        - TaskEntity
      eventHandlers:
        - event: "diag_snp(u32)"
          handler: handleStateTransitionSnapshot
        - event: "diag_cor(u32)"
          handler: handleCorrelatedOperation
```
