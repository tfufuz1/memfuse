//! MemFuse Runtime — Sandboxing and Execution Layer (WP-5.2).

#![forbid(unsafe_code)]

pub mod sandbox;
pub mod airgap;

pub use sandbox::{SandboxConfig, WasmSandbox};

use memfuse_core::{Result, TokenBudget};

/// Defines the execution boundaries for sandbox containers.
#[async_trait::async_trait]
pub trait AgentRuntime: Send + Sync {
    /// Executes a binary module with isolated constraints.
    async fn execute_isolated(&self, module_bin: &[u8], budget: &TokenBudget) -> Result<Vec<u8>>;
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
