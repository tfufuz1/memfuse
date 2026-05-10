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
//! This crate provides the execution environment for agent tools and untrusted code.
//! It focuses on security and isolation using WebAssembly (WASM) sandboxing.
//!
//! # Architecture Role (Cockpit — Layer 3)
//!
//! The Runtime layer ensures that agents can execute tools (like Python scripts,
//! shell commands, or specialized WASM modules) without compromising the host system.
//!
//! Key components:
//! - [`WasmSandbox`]: The primary interface for executing WASM-based tools.
//! - [`SandboxConfig`]: Configuration for resource limits and capabilities.

#![forbid(unsafe_code)]

pub mod sandbox;

pub use sandbox::{SandboxConfig, WasmSandbox};
