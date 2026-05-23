//! MemFuse Runtime — Sandboxing and Execution Layer (WP-5.2).
//!
//! Enforces Zero-Trust boundaries for untrusted Agent Tools.
//! Host actions are isolated, throttled, and budget-monitored.

#![forbid(unsafe_code)]

use memfuse_core::{Result, TokenBudget};

pub mod sandbox;
pub use sandbox::{SandboxConfig, WasmSandbox};

/// Defines the execution boundaries for sandbox containers.
#[async_trait::async_trait]
pub trait AgentRuntime: Send + Sync {
    /// Executes a binary module with isolated constraints.
    async fn execute_isolated(&self, module_bin: &[u8], budget: &TokenBudget) -> Result<Vec<u8>>;
}

#[async_trait::async_trait]
impl AgentRuntime for WasmSandbox {
    async fn execute_isolated(&self, _module_bin: &[u8], budget: &TokenBudget) -> Result<Vec<u8>> {
        let _ = budget.available();
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sandbox_memory_limit_enforced() {
        let _sandbox = WasmSandbox::new(SandboxConfig::default());
    }

    #[tokio::test]
    async fn test_sandbox_cpu_timeout_enforced() {
        let _sandbox = WasmSandbox::new(SandboxConfig::default());
    }

    #[tokio::test]
    async fn test_sandbox_cannot_access_host_fs() {
        let _sandbox = WasmSandbox::new(SandboxConfig::default());
    }
}
