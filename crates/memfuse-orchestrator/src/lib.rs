//! # MemFuse Orchestrator — Agent Workflow Engine
//!
//! This crate provides the high-level orchestration layer for MemFuse, enabling
//! the definition and execution of complex agent workflows using declarative StateGraphs.
//!
//! ## Core Concepts
//! - **StateGraph**: A directed graph where nodes represent agent tasks and edges represent transitions.
//! - **Agent Execution**: Autonomous execution of workflows in an isolated environment.

// ANCHOR:DOC:DOC-LIB-001 — Missing module documentation
// WP:WP-0.0 PRIO:3 NEEDS:NONE
// AGENT:13 DATE:2026-05-09 STATUS:READY
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
// ANCHOR:INTEGRATION STATUS:TODO AGENT:13

#![forbid(unsafe_code)]

pub mod graph;

pub use graph::{AgentNode, StateGraph};
