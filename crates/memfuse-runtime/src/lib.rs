//! MemFuse Runtime — Sandboxing and Execution Layer.
// ANCHOR:DOC:DOC-LIB-001 — Missing module documentation
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
// ANCHOR:INTEGRATION PRIO:2 STATUS:DONE AGENT:12 DATE:2026-05-18
// DONE: Cross-Crate Integration Tests für WASM-Sandbox Isolation und Tool-Execution.

#![forbid(unsafe_code)]

pub mod sandbox;

pub use sandbox::{SandboxConfig, WasmSandbox};
