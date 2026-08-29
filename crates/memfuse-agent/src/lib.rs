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
//! # Invariants
//! 1. **Auto-checkpoint before step**: Creates a checkpoint before executing each node handler,
//!    backed by RAII `CheckpointGuard` for automatic transaction rollback upon failure.
//! 2. **Deterministic replay & rollback**: Restores `AgentContext` to any prior checkpoint step.
//! 3. **Immutable audit trail**: Appends step records to state collection under `audit:{task_id}:step:{n}`.
//! 4. **Token budget limit**: Enforces token budget consumption on each step.
//!
//! ADR-020: Re-integration from archived `memfuse-saos-agent` (Commit ddc4c77).
// AI-TAG[DOC-DRIFT][MINOR] RESOLVED: AGT-AGENT-001 — Re-extracted workflow engine crate requires integration verification. (TS:2026-08-27T00:00:00Z)

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
