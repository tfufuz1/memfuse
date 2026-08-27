# memfuse-agent

Persistent agent workflow engine for MemFuse — `checkpoint → execute → commit → audit` loop.

## Overview

`memfuse-agent` provides a pure Rust, sovereign orchestrator engine for multi-step AI agent workflows without external dependencies like LangGraph or AutoGen.

## Architectural Position (Layer 3)

`memfuse-agent` operates at Layer 3 of the MemFuse DAG:

- **Depends on**:
  - `memfuse-core`: Result types, errors, `TokenBudget`, `StorageEngine` trait.
  - `memfuse-store`: `LsmStorage` persistence backend.
  - `memfuse-checkpoint`: Snapshot persistence (`PersistentCheckpointStore`) & RAII `CheckpointGuard`.
  - `memfuse-graph`: `StateGraph`, `AgentNode`, `NodeType`, `WorkflowEdge`.
  - `memfuse-db`: `MemFuse` engine, `Collection` document/vector storage.

## Invariants & Core Loop

1. **AC-1: Auto-checkpoint before step**:
   Before executing any node handler, a snapshot checkpoint is stored in `PersistentCheckpointStore`, and a RAII `CheckpointGuard` is initialized. If an error occurs during execution, dropping the guard triggers an automatic transaction rollback to preserve state consistency.
2. **AC-2: Deterministic replay & rollback**:
   State checkpoints support restoring `AgentContext` (`current_node`, `step_count`, `memory`) to any step index or node identifier.
3. **AC-3: Immutable audit log**:
   All step executions append immutable `AuditEntry` records keyed by `audit:{task_id}:step:{step_count}` into the agent state collection. No delete/update paths exist.
4. **Token Budget Enforcement**:
   Steps consume tokens from `TokenBudget`. Budget exhaustion immediately halts execution and returns a `MemFuseError::Internal`.

## Usage Example

```rust
use memfuse_agent::{AgentContext, AgentTool, NodeType, OrchestratorEngine, StateGraph, StepResult};
use memfuse_core::TokenBudget;
use memfuse_db::{MemFuse, MemFuseConfig};
use std::sync::Arc;

let db = Arc::new(MemFuse::open_with_config(path, config).await?);
let state_col = db.collection("agent_state").await?;
let mut ctx = AgentContext::new("task-100", "start", db.clone(), state_col, TokenBudget::new(1000, 0));

let mut graph = StateGraph::new();
graph.add_node("start", "Start Node", NodeType::Start, None);
graph.add_node("end", "End Node", NodeType::End, None);
graph.add_edge("start", "end", None, 1);

let engine = OrchestratorEngine::new(db.inner_storage());
engine.run(&mut ctx, &graph).await?;
```
