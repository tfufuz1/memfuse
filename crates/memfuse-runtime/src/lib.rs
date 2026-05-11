// ANCHOR:DOC:DOC-LIB-001 — Missing module documentation
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
//! MemFuse Runtime — Sandboxing and Execution Layer.
//!
//! # Architecture Role (Cockpit — Layer 3)
//!
//! The Runtime layer provides a secure, isolated environment for executing
//! untrusted agent code or tools. It ensures that the execution environment
//! is decoupled from the host system, preventing unauthorized access.
//!
//! Key components:
//! - **WasmSandbox**: A WebAssembly-based execution environment using `wasmtime`.
//! - **SandboxConfig**: Configurable resource limits (memory, fuel) for sandboxes.

#![forbid(unsafe_code)]

pub mod sandbox;

pub use sandbox::{SandboxConfig, WasmSandbox};
