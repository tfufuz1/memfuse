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
// ANCHOR:INTEGRATION STATUS:DONE AGENT:13
//! MemFuse Orchestrator — Declarative StateGraphs and Agent execution.
//!
//! This crate implements a declarative workflow engine based on StateGraphs.
//! It allows developers to define complex agent behaviors as a series of
//! interconnected nodes and conditional transitions.
//!
//! ## Orchestration Logic
//! - **StateGraph**: A directed graph where nodes represent agent tasks or tools.
//! - **Transitions**: Edges define the flow between nodes, optionally guarded by conditions.
//! - **Context Management**: Orchestrates the flow of information between sandboxed
//!   execution nodes and the MemFuse hybrid-search core.

#![forbid(unsafe_code)]

pub mod graph;

pub use graph::{AgentNode, StateGraph};
