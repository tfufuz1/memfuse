# memfuse-agent — Persistent Agent Workflow Engine Rules

## Critical Invariants

### Checkpoint-Execute-Commit-Audit Execution Loop
- The agent orchestrator enforces a strict workflow loop: Start/Task node -> auto Checkpoint via `CheckpointGuard` -> execute handler -> persist state/flush LSM -> auto Audit entry `audit:{task_id}:step:{n}` -> resolve edge.
- State transitions follow `Idle` -> `Running` -> (`Checkpoint` -> `Execute` -> `Commit` -> `Audit`)* -> `Completed` | `Failed`.
- Panic/error dropping of active `CheckpointGuard` triggers automatic rollback via `rollback_to_tx`.

### StateGraph vs. CsrGraph
- `memfuse-agent` defines its own declarative workflow graph (`StateGraph`) for step routing and tool execution.
- `StateGraph` is NOT the CSR graph index in `memfuse-graph::csr`.

### Deterministic Replay & Immutability
- Context state replay (`replay_from`) reconstructs workflow progress from persisted LSM checkpoints.
- Audit trail entries (`AuditLog`) are append-only; no deletion or mutation path exists by design.

### Resource Limits & Bounds
- Pending background events in `PollingDocumentEventSource` & telemetry in `AgentContext` are strictly bounded by `MAX_EVENT_SOURCE_CAPACITY` and `MAX_TELEMETRY_EVENTS` (10,000 items) to prevent memory exhaustion.
- Identifiers (`task_id`, `node_id`, tool names) are validated to be non-empty, <= 256 bytes, and null-byte free.

## Layer Position
Layer 3. May import: `memfuse-db` (L2), `memfuse-checkpoint` (L1), `memfuse-graph` (L1), `memfuse-store` (L1), `memfuse-core` (L0). Must NOT import: `memfuse-tauri` (L4).
