//! MemFuse Runtime — Sandboxing and Execution Layer (WP-5.2).

#![forbid(unsafe_code)]

pub mod sandbox;
pub use sandbox::{SandboxConfig, WasmSandbox};

use memfuse_core::{Result, TokenBudget};

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
