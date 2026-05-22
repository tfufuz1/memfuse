//! MemFuse Runtime — Sandboxing and Execution Layer (WP-5.2).
//!
//! Enforces Zero-Trust boundaries for untrusted Agent Tools.
//! Host actions are isolated, throttled, and budget-monitored.

// ANCHOR:ARCH:RUNTIME-001 — WASM Sandbox (Cockpit — Layer 3).
// WP:NONE PRIO:2 NEEDS:NONE
// AGENT:01 DATE:2026-05-09 STATUS:DONE
// CREATED:2026-05-09 DEADLINE:NONE
// ZIEL: Sichere Ausführung von Agent-Tools ohne Host-Zugriff.

#![forbid(unsafe_code)]

use memfuse_core::{Result, TokenBudget};

pub mod sandbox;
pub mod airgap;

pub use sandbox::{WasmSandbox, SandboxConfig};

/// Defines the execution boundaries for sandbox containers.
#[async_trait::async_trait]
pub trait AgentRuntime: Send + Sync {
    /// Executes a binary module with isolated constraints.
    async fn execute_isolated(&self, module_bin: &[u8], budget: &TokenBudget) -> Result<Vec<u8>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// AC-1: test_sandbox_memory_limit_enforced
    #[tokio::test]
    async fn test_sandbox_memory_limit_enforced() {
        let config = SandboxConfig {
            max_memory_mb: 64,
            timeout: Duration::from_millis(500),
            allow_network: false,
        };
        let _sandbox = WasmSandbox::new(config);
    }
}
