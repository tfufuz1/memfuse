// ANCHOR:AUDIT:SAOS-023 — forbid(unsafe_code) fehlte → nachgerüstet
// AGENT:saos-audit DATE:2026-05-08 STATUS:FIXED
//! MemFuse Orchestrator — Declarative StateGraphs and Agent execution.

#![forbid(unsafe_code)]

pub mod graph;

pub use graph::{AgentNode, StateGraph};
