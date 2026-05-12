// ANCHOR:DOC:DOC-LIB-001 — Module documentation added
// WP:WP-0.0 PRIO:3 NEEDS:NONE
// AGENT:13 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-09 DEADLINE:NONE
// ANCHOR:AUDIT:SAOS-022 — forbid(unsafe_code) fehlte → nachgerüstet
// WP:NONE PRIO:3 NEEDS:NONE
// AGENT:NONE DATE:2026-05-09 STATUS:READY
// CREATED:2026-05-09 DEADLINE:NONE
// AGENT:saos-audit DATE:2026-05-08 STATUS:FIXED
//
// ANCHOR:ARCH:RUNTIME-001 — WASM Sandbox (Cockpit — Layer 3).
// WP:NONE PRIO:2 NEEDS:NONE
// AGENT:NONE DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-09 DEADLINE:NONE
// ZIEL: Sichere Ausführung von Agent-Tools ohne Host-Zugriff.
// ANCHOR:INTEGRATION STATUS:TODO AGENT:13
//! MemFuse Runtime — Sandboxing and Execution Layer.
//!
//! This crate provides the execution environment for Agent tools and workflows.
//! It focuses on secure execution using WASM-based sandboxing, ensuring that
//! arbitrary code can be run safely without compromising the host system.

#![forbid(unsafe_code)]

pub mod sandbox;

pub use sandbox::{SandboxConfig, WasmSandbox};
