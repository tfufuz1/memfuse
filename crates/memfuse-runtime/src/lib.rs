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
// ANCHOR:INTEGRATION STATUS:DONE AGENT:13
//! MemFuse Runtime — Sandboxing and Execution Layer.
//!
//! This crate provides a secure execution environment for untrusted agent tools
//! using WebAssembly (WASM) sandboxing. It ensures that any code executed by
//! agents is strictly isolated from the host system.
//!
//! ## Sandboxing Mechanism
//! - **Isolierte Ausführung**: Tools run in a dedicated WASM virtual machine.
//! - **Ressourcen-Beschränkung**: Configurable limits for memory usage and execution time.
//! - **Network Isolation**: By default, sandboxed code has no access to the network.

#![forbid(unsafe_code)]

pub mod sandbox;

pub use sandbox::{SandboxConfig, WasmSandbox};
