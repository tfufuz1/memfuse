//! WebAssembly Sandboxing for safe agent tool execution.
//!
//! Enforces Zero-Trust boundaries around untrusted Agent Tools.
//! Host actions are isolated, throttled, and budget-monitored.

use memfuse_core::error::MemFuseError;
use memfuse_core::Result;
use std::time::Duration;
use wasmtime::{Config, Engine, StoreLimits, StoreLimitsBuilder};

#[derive(Debug)]
pub struct SandboxConfig {
    pub max_memory_mb: usize,
    pub timeout: Duration,
    pub allow_network: bool,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            max_memory_mb: 64,
            timeout: Duration::from_millis(500),
            allow_network: false,
        }
    }
}

/// Executes arbitrary WASM payloads isolated from the host.
pub struct WasmSandbox {
    config: SandboxConfig,
    pub(crate) engine: Engine,
}

impl WasmSandbox {
    /// Creates a new WASM sandbox with the given configuration.
    pub fn new(config: SandboxConfig) -> Result<Self> {
        let mut engine_config = Config::new();

        // AC-2: CPU Fuel Consumption (Fuel represents execution time/steps)
        engine_config.consume_fuel(true);
        engine_config.async_support(true);

        let engine =
            Engine::new(&engine_config).map_err(|e| MemFuseError::Sandbox(e.to_string()))?;

        Ok(Self { config, engine })
    }

    /// Provides StoreLimits configured per SandboxConfig. (AC-1)
    pub(crate) fn build_store_limits(&self) -> StoreLimits {
        StoreLimitsBuilder::new()
            .memory_size(self.config.max_memory_mb * 1024 * 1024)
            .build()
    }

    /// Configures fuel based on timeout heuristic.
    pub(crate) fn max_fuel(&self) -> u64 {
        // Rough heuristic: 1 ms = 10_000 fuel units
        (self.config.timeout.as_millis() as u64) * 10_000
    }
}
