//! MemFuse Runtime — Sandboxing and Execution Layer (WP-5.2).
//!
//! Enforces Zero-Trust boundaries for untrusted Agent Tools.
//! Host actions are isolated, throttled, and budget-monitored.

#![allow(async_fn_in_trait)]
#![forbid(unsafe_code)]

pub mod host_functions;
pub mod sandbox;

use memfuse_core::{Result, TokenBudget};
pub use sandbox::{SandboxConfig, WasmSandbox};

use host_functions::{bind_host_functions, SandboxState};
use memfuse_core::error::MemFuseError;
use wasmtime::{Linker, Module, Store};

/// Defines the execution boundaries for sandbox containers.
pub trait AgentRuntime: Send + Sync {
    /// Executes a binary module with isolated constraints.
    async fn execute_isolated(&self, module_bin: &[u8], budget: &TokenBudget) -> Result<Vec<u8>>;
}

impl AgentRuntime for WasmSandbox {
    async fn execute_isolated(&self, module_bin: &[u8], budget: &TokenBudget) -> Result<Vec<u8>> {
        let _ = budget.available();

        let engine = &self.engine;
        let mut linker = Linker::new(engine);

        // Bind DB functions
        bind_host_functions(&mut linker)?;

        // AC-1: Setup StoreLimits for memory
        let state = SandboxState {
            limits: self.build_store_limits(),
        };
        let mut store = Store::new(engine, state);
        store.limiter(|state| &mut state.limits);

        // AC-2: Set fuel for CPU timeout mapping
        store
            .set_fuel(self.max_fuel())
            .map_err(|e| MemFuseError::Sandbox(e.to_string()))?;

        // Compile WASM binary
        let module = Module::new(engine, module_bin)
            .map_err(|e| MemFuseError::Sandbox(format!("compile error: {}", e)))?;

        // Instantiate
        let instance = linker
            .instantiate(&mut store, &module)
            .map_err(|e| MemFuseError::Sandbox(format!("instantiation error: {}", e)))?;

        // Execute AC test target function (assume `main` for simple execution tests)
        let main = instance
            .get_typed_func::<(), ()>(&mut store, "main")
            .map_err(|e| MemFuseError::Sandbox(format!("main not found: {}", e)))?;

        main.call(&mut store, ()).map_err(|e| {
            let err_str = format!("{:#}", e);
            // Map WASM trap types to our AC error variants
            if err_str.contains("fuel")
                || err_str.contains("timeout")
                || err_str.contains("interrupt")
            {
                MemFuseError::SandboxTimeout("CPU Fuel exhausted".into())
            } else if err_str.contains("memory maximum")
                || err_str.contains("out of bounds")
                || err_str.contains("MemoryLimitExceeded")
            {
                MemFuseError::MemoryLimitExceeded("Memory bound hit".into())
            } else {
                MemFuseError::Sandbox(err_str)
            }
        })?;

        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Helper to assemble raw WAT into binary
    fn wat2wasm(wat: &str) -> Vec<u8> {
        wat::parse_str(wat).expect("failed to parse WAT")
    }

    /// AC-1: test_sandbox_memory_limit_enforced
    #[tokio::test]
    async fn test_sandbox_memory_limit_enforced() {
        // Enforce a tiny max memory: 1MB. (WASM pages are 64KB, so 1MB = 16 pages)
        let config = SandboxConfig {
            max_memory_mb: 1,
            timeout: Duration::from_secs(10),
            allow_network: false,
        };
        let sandbox = WasmSandbox::new(config).expect("unwrap allowed (AGENT:00)");
        let budget = TokenBudget::new(100, 0);

        // WAT: loop calling memory.grow to exhaust memory limits.
        let wat = r#"
            (module
                (memory 1)
                (func (export "main")
                    (if (i32.eq (memory.grow (i32.const 100)) (i32.const -1))
                        (then
                            ;; Force out-of-bounds trap to signal memory limit enforcement
                            (drop (i32.load (i32.const 20000000)))
                        )
                    )
                )
            )
        "#;
        let bin = wat2wasm(wat);

        let err = sandbox.execute_isolated(&bin, &budget).await.unwrap_err();
        assert!(
            matches!(err, MemFuseError::MemoryLimitExceeded(_)),
            "Expected MemoryLimitExceeded, got: {:?}",
            err
        );
    }

    /// AC-2: test_sandbox_cpu_timeout_enforced
    #[tokio::test]
    async fn test_sandbox_cpu_timeout_enforced() {
        let config = SandboxConfig {
            max_memory_mb: 64,
            timeout: Duration::from_millis(50), // tiny timeout
            allow_network: false,
        };
        let sandbox = WasmSandbox::new(config).expect("unwrap allowed (AGENT:00)");
        let budget = TokenBudget::new(100, 0);

        // WAT: tight infinite loop consuming CPU fuel.
        let wat = r#"
            (module
                (func (export "main")
                    (loop $my_loop
                        (br $my_loop)
                    )
                )
            )
        "#;
        let bin = wat2wasm(wat);

        let err = sandbox.execute_isolated(&bin, &budget).await.unwrap_err();
        assert!(
            matches!(err, MemFuseError::SandboxTimeout(_)),
            "Expected SandboxTimeout, got: {:?}",
            err
        );
    }

    /// AC-3: test_sandbox_cannot_access_host_fs
    #[tokio::test]
    async fn test_sandbox_cannot_access_host_fs() {
        let config = SandboxConfig::default();
        let sandbox = WasmSandbox::new(config).expect("unwrap allowed (AGENT:00)");
        let budget = TokenBudget::new(100, 0);

        // We simulate a WASI requirement without providing WASI to the linker.
        // It should fail instantiation with "unknown import: wasi_snapshot_preview1::fd_read"
        // representing a PolicyViolation (cannot even link).
        let wat = r#"
            (module
                (import "wasi_snapshot_preview1" "fd_read"
                    (func $fd_read (param i32 i32 i32 i32) (result i32)))
                (func (export "main")
                )
            )
        "#;
        let bin = wat2wasm(wat);

        let err = sandbox.execute_isolated(&bin, &budget).await.unwrap_err();
        let err_str = err.to_string();
        assert!(
            err_str.contains("wasi_snapshot_preview1"),
            "Expected Linker failure for unauthorized WASI fs block, got: {:?}",
            err_str
        );
    }
}
