# memfuse-agent — Crate-Level Agent Rules

## Critical Invariants

### Checkpoint-Execute-Commit-Audit Loop
- Enforces workflow loop: Start/Task node -> auto Checkpoint via `CheckpointGuard` -> execute handler -> persist state/flush LSM -> auto Audit entry `audit:{task_id}:step:{n}` -> resolve edge.
- Idle -> Running -> (Checkpoint -> Execute -> Commit -> Audit)* -> Completed | Failed state machine.
- Rollback on panic/error drops active `CheckpointGuard` for automatic transaction rollback via `rollback_to_tx`.

### Deterministic Replay & Audit Trail
- Replays context state via `OrchestratorEngine::replay_from()` from prior checkpoints.
- Append-only audit entries stored in LSM collection without deletion path.

## Layer Position
Layer 3. Darf importieren: memfuse-db (L2), memfuse-checkpoint (L1), memfuse-graph (L1), memfuse-core (L0). Darf NICHT importieren: memfuse-tauri (L4).

## Nicht-offensichtliche Entscheidungen
- `inner_storage()` access is restricted strictly to `PersistentCheckpointStore` & `CheckpointGuard`.
- StateGraph and context management decouple node step resolution from direct graph crate coupling in production.
