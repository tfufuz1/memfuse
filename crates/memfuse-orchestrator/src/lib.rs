// ANCHOR:DOC:DOC-LIB-001 — Missing module documentation
// WP:WP-0.0 PRIO:3 NEEDS:NONE
// AGENT:13 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-09 DEADLINE:NONE
// ANCHOR:AUDIT:SAOS-023 — forbid(unsafe_code) fehlte → nachgerüstet
// WP:NONE PRIO:3 NEEDS:NONE
// AGENT:NONE DATE:2026-05-09 STATUS:READY
// CREATED:2026-05-09 DEADLINE:NONE
// AGENT:saos-audit DATE:2026-05-08 STATUS:FIXED
//
// ANCHOR:ARCH:ORCHESTRATOR-001 — Agent Workflow Engine (Cockpit — Layer 3).
// WP:NONE PRIO:2 NEEDS:NONE
// AGENT:NONE DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-09 DEADLINE:NONE
// ZIEL: Deklarative LangGraph-ähnliche Graphenausführung in nativem Rust.
//! MemFuse Orchestrator — Declarative StateGraphs and Agent execution.
//!
//! # Architecture Role (Cockpit — Layer 3)
//!
//! The Orchestrator is responsible for managing complex agentic workflows
//! using a declarative graph-based approach. It coordinates between various
//! nodes, managing state transitions and ensuring reliable execution of
//! long-running processes.
//!
//! Key components:
//! - **StateGraph**: A directed acyclic graph representing a workflow.
//! - **AgentNode**: A unit of work within a StateGraph.

#![forbid(unsafe_code)]

pub mod graph;

pub use graph::{AgentNode, StateGraph};
