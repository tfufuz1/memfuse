//! MemFuse Runtime — Sandboxing and Execution Layer (WP-5.2).
//!
//! Enforces Zero-Trust boundaries for untrusted Agent Tools.
//! Host actions are isolated, throttled, and budget-monitored.

#![forbid(unsafe_code)]

pub mod airgap;
pub mod sandbox;

pub use airgap::*;
pub use sandbox::*;

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
        // TODO(WP-5.2): Bind to wasmtime engine, applying TokenBudget bounds.
        let _ = budget.available();
        Ok(Vec::new())
    }
}
