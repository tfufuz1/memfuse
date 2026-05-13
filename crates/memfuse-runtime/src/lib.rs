// ANCHOR:DOC:DOC-LIB-001 — Module documentation improved
// WP:WP-0.0 PRIO:3 NEEDS:NONE
// AGENT:13 DATE:2026-05-13 STATUS:DONE
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
//! # MemFuse Runtime
//!
//! The Runtime crate provides sandboxing and execution capabilities for MemFuse agents.
//! It ensures that agent tools and custom logic are executed in a secure, isolated
//! environment, typically using WebAssembly (WASM).
//!
//! ## Key Responsibilities
//! - **Isolation**: Executing code without direct access to the host system.
//! - **Resource Control**: Managing CPU and memory limits for execution.
//! - **Portability**: Enabling agent logic to run across different platforms via WASM.

#![forbid(unsafe_code)]

pub mod sandbox;

pub use sandbox::{SandboxConfig, WasmSandbox};
