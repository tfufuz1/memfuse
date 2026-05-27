//! Host Functions exposed to the WASM Sandbox.
//!
//! Provides restrictive bindings for `db_search`, `db_insert`, and `db_get`.

use memfuse_core::error::MemFuseError;
use wasmtime::{Caller, Linker, StoreLimits};

/// Context state held by the Store.
pub struct SandboxState {
    pub limits: StoreLimits,
}

impl wasmtime::ResourceLimiter for SandboxState {
    fn memory_growing(
        &mut self,
        current: usize,
        desired: usize,
        maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        self.limits.memory_growing(current, desired, maximum)
    }

    fn table_growing(
        &mut self,
        current: u32,
        desired: u32,
        maximum: Option<u32>,
    ) -> wasmtime::Result<bool> {
        self.limits.table_growing(current, desired, maximum)
    }
}

/// Binds the host functions to the linker.
pub fn bind_host_functions(linker: &mut Linker<SandboxState>) -> memfuse_core::Result<()> {
    linker
        .func_wrap(
            "env",
            // TODO(FIND-SBX-001): Skeleton Implementierung in Host-Funktionen (WP-6)
            // Implement real async DB bindings via channels instead of returning 0.
            "db_search",
            |_caller: Caller<'_, SandboxState>, _query_ptr: i32, _query_len: i32, _k: i32| -> i32 {
                // TODO(WP-6): Actual orchestrator L2 loopback
                0
            },
        )
        .map_err(|e| MemFuseError::Sandbox(e.to_string()))?;

    linker
        .func_wrap(
            "env",
            "db_insert",
            |_caller: Caller<'_, SandboxState>,
             _key_ptr: i32,
             _key_len: i32,
             _vec_ptr: i32,
             _vec_len: i32|
             -> i32 { 0 },
        )
        .map_err(|e| MemFuseError::Sandbox(e.to_string()))?;

    linker
        .func_wrap(
            "env",
            "db_get",
            |_caller: Caller<'_, SandboxState>, _key_ptr: i32, _key_len: i32| -> i32 { 0 },
        )
        .map_err(|e| MemFuseError::Sandbox(e.to_string()))?;

    Ok(())
}
