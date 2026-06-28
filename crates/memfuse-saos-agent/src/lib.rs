//! MemFuse Orchestrator — Declarative StateGraphs and Agent execution (WP-5.3).
//!
//! Sovereign, declarative alternative to LangGraph/AutoGen.
//! Constructs acyclic and dynamic graphs routing autonomous agent steps.
// TODO: Missing module documentation
// INTENT: forbid(unsafe_code) fehlte → nachgerüstet
//
// INVARIANT: Agent Workflow Engine (Cockpit — Layer 3).
// ZIEL: Deklarative LangGraph-ähnliche Graphenausführung in nativem Rust.
// ANCHOR:INTEGRATION PRIO:2 STATUS:DONE AGENT:07 DATE:2026-05-20
// ANCHOR:INTEGRATION PRIO:2 STATUS:DONE AGENT:07 DATE:2026-05-20
// DONE: Cross-Crate Integration Tests für StateGraph und Agent-Interaktion implementiert.

#![allow(async_fn_in_trait)]
#![forbid(unsafe_code)]

pub mod audit;
pub mod context;
pub mod engine;
pub mod graph;
pub mod step;

pub use context::AgentContext;
pub use engine::OrchestratorEngine;
pub use graph::{AgentNode, NodeType, StateGraph, WorkflowEdge};
pub use step::{AgentTool, StepResult};
