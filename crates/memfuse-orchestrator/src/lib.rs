// ANCHOR:AUDIT:SAOS-023 — forbid(unsafe_code) fehlte → nachgerüstet
// AGENT:saos-audit DATE:2026-05-08 STATUS:FIXED
//
// ANCHOR:ARCH:ORCHESTRATOR-001 — Agent Workflow Engine (Cockpit — Layer 3).
// ZIEL: Deklarative LangGraph-ähnliche Graphenausführung in nativem Rust.
//! MemFuse Orchestrator — Declarative StateGraphs and Agent execution.

#![forbid(unsafe_code)]

pub mod graph;

pub use graph::{AgentNode, StateGraph};
