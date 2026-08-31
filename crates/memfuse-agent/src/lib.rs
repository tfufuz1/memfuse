// FILE-CONTEXT Header (Format v3)
// ZWECK: Crate entry point exposing audit, context, engine, event_source, graph, and step submodules.
// INVARIANTEN: Re-exports core workflow primitives; Layer 3 orchestrator integration for MemFuse.
// NICHT-OFFENSICHTLICH: Preserves public API boundaries for persistent checkpoint-execute-commit-audit loops.
// HOTSPOTS: Module re-exports (ll. 75-85).
// STAND: TS:2026-08-31T21:07:58Z (SESSION: 5f1a7b8e)

//! MemFuse Agent — Persistent workflow engine for multi-step agent execution.
//!
//! Implements the deterministic `checkpoint → execute → commit → audit` loop.
//! Sovereign alternative to LangGraph/AutoGen: pure Rust, zero external dependencies.
//!
//! # Architecture Role (Layer 3)
//! Built upon `memfuse-db` (Collections), `memfuse-checkpoint` (Snapshots & RAII CheckpointGuard),
//! `memfuse-graph` (Declarative StateGraph), and `memfuse-store` (LSM Storage). Manages workflow
//! state, token budget enforcement, and immutable audit logging over LSM-persisted keys.
//!
//! # State Machine Diagram & Invariants
//! ```text
//!              +--------+
//!              |  Idle  |
//!              +---+----+
//!                  | run()
//!                  v
//!            +-----+------+
//!            |  Running   | <---+ (Loop: Checkpoint -> Execute -> Commit -> Audit)
//!            +--+------+--+     |
//!               |      |        |
//!        (NodeEnd)    (Error/  ---+
//!               |     Panic)
//!               v      v
//!         +-----+--+ +-+------+
//!         |Completed| | Failed |
//!         +--------+ +--------+
//! ```
//!
//! ### Enforced State Transitions:
//! - **Idle -> Running**: Initiated at entry of `OrchestratorEngine::run()`.
//! - **Running -> Running**: Step execution loop enforced via `CheckpointGuard::for_agent_step` before node handler execution.
//!   Execution cannot proceed without completing the preceding checkpoint.
//! - **Running -> Completed**: Triggered on `NodeType::End` node arrival after flushing LSM storage.
//! - **Running -> Failed**: Triggered on any step error (tool execution error, budget exhaustion, unresolved edge, missing handler).
//!
//! ### Crash Recovery Behavior during `execute()`:
//! If a crash or panic occurs after a checkpoint is written but before step execution finishes:
//! 1. The active `CheckpointGuard` is dropped, triggering automatic transaction rollback via `rollback_to_tx`.
//! 2. On application restart, `OrchestratorEngine::replay_from()` restores `AgentContext` state to the last valid checkpoint step.
//!
//! ### Cross-Layer Coupling Analysis:
//! - `memfuse-agent` depends on `memfuse-db` for `Collection` and high-level operations.
//! - Direct calls to `memfuse-store` (`inner_storage()`) are strictly isolated to `PersistentCheckpointStore` and `CheckpointGuard` for transaction ID allocation and checkpoint snapshot manipulation, which `memfuse-db` intentionally does not expose.
//! - Production code (`src/`) contains **zero** direct calls to `memfuse-graph`; graph state operations pass exclusively through `StateGraph` struct and `memfuse-db`.
//!
//! ### Audit Trail Integrity:
//! - Immutable append-only audit entries are stored under `audit:{task_id}:step:{n}` via `Collection::insert`.
//! - Entries pass directly through LSM storage and are protected by the same WAL-HMAC integrity chain (`memfuse-crypto`) as all standard storage operations.
//! - Failed executions log an `AuditEntry` with `error` details before transitioning context to `AgentStatus::Failed`.
//!
//! # Invariants
//! 1. **Auto-checkpoint before step**: Creates a checkpoint before executing each node handler,
//!    backed by RAII `CheckpointGuard` for automatic transaction rollback upon failure.
//! 2. **Deterministic replay & rollback**: Restores `AgentContext` to any prior checkpoint step.
//! 3. **Immutable audit trail**: Appends step records to state collection under `audit:{task_id}:step:{n}`.
//! 4. **Token budget limit**: Enforces token budget consumption on each step.
//!
//! ADR-042: Re-integration from archived `memfuse-saos-agent` (Commit ddc4c77).
// AI-TAG[DOC-DRIFT][MINOR] RESOLVED: AGT-AGENT-001 — Re-extracted workflow engine crate requires integration verification. (TS:2026-08-31T21:07:58Z) (SESSION: 5f1a7b8e)

#![forbid(unsafe_code)]
#![allow(async_fn_in_trait)]

pub mod audit;
pub mod context;
pub mod engine;
pub mod event_source;
pub mod graph;
pub mod step;

pub use context::{AgentContext, AgentStatus};
pub use engine::{EventLoopExitReason, OrchestratorEngine};
pub use event_source::{BackgroundEvent, EventSource, PollingDocumentEventSource, VecEventSource};
pub use graph::{AgentNode, NodeType, StateGraph, WorkflowEdge};
pub use step::{AgentTool, StepResult};
