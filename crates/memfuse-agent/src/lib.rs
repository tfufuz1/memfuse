//! MemFuse Agent — Persistenter Workflow-Engine für Multi-Step Agenten-Ausführung.
//!
//! Implementiert den deterministischen `checkpoint → execute → commit → audit`-Loop.
//! Souveräne Alternative zu LangGraph/AutoGen: pure Rust, keine externen Abhängigkeiten.
//!
//! # Architektur-Rolle (Cockpit — Layer 3)
//! Aufgesetzt auf `memfuse-db` (Collections), `memfuse-checkpoint` (MVCC-Snapshots)
//! und `memfuse-graph` (Session-DAG). Verwaltet Workflow-State, Token-Budget und
//! Audit-Log über LSM-persistierte Keys.
//!
//! ADR-020: Wiederherstellung aus gelöschtem `memfuse-saos-agent` (Commit ddc4c77).

#![forbid(unsafe_code)]
#![allow(async_fn_in_trait)]

pub mod audit;
pub mod context;
pub mod engine;
pub mod graph;
pub mod step;

pub use context::{AgentContext, AgentStatus};
pub use engine::OrchestratorEngine;
pub use graph::{AgentNode, NodeType, StateGraph, WorkflowEdge};
pub use step::{AgentTool, StepResult};
