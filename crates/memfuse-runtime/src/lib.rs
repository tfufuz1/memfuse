// ANCHOR:AUDIT:SAOS-022 — forbid(unsafe_code) fehlte → nachgerüstet
// AGENT:saos-audit DATE:2026-05-08 STATUS:FIXED
//! MemFuse Runtime — Sandboxing and Execution Layer.

#![forbid(unsafe_code)]

pub mod sandbox;

pub use sandbox::{SandboxConfig, WasmSandbox};
