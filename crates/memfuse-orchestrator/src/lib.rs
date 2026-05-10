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
//! This crate implements the workflow engine for multi-agent systems.
//! It uses a graph-based approach to define agent interactions and state transitions.
//!
//! # Architecture Role (Cockpit — Layer 3)
//!
//! The Orchestrator sits at the top of the stack, coordinating between the
//! database layer (Layer 1/2) and the execution runtime (Layer 3).
//!
//! Key concepts:
//! - [`StateGraph`]: A directed graph defining the flow of execution.
//! - [`AgentNode`]: A unit of execution within the graph, typically representing an LLM or tool call.

#![forbid(unsafe_code)]

pub mod graph;

pub use graph::{AgentNode, StateGraph};
