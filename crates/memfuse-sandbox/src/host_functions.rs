//! Host Functions exposed to the WASM Sandbox.
//!
//! Provides restrictive bindings for `db_search`, `db_insert`, and `db_get`.

use memfuse_core::error::MemFuseError;
use wasmtime::{Caller, Linker, StoreLimits};

/// Context state held by the Store.
pub struct SandboxState {
    pub limits: StoreLimits,
    pub bridge: Option<std::sync::Arc<dyn crate::SandboxBridge>>,
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
    // db_search(query_ptr, query_len, k) -> i32 (result_len or error)
    linker
        .func_wrap3_async(
            "env",
            "db_search",
            |mut caller: Caller<'_, SandboxState>, query_ptr: i32, query_len: i32, k: i32| {
                Box::new(async move {
                    let _memory = match caller.get_export("memory") {
                        Some(m) => m
                            .into_memory()
                            .ok_or_else(|| wasmtime::Error::msg("failed to get memory export"))?,
                        None => return Err(wasmtime::Error::msg("memory not exported")),
                    };

                    let mut query = vec![0u8; query_len as usize];
                    _memory.read(&caller, query_ptr as usize, &mut query)?;

                    let bridge = match &caller.data().bridge {
                        Some(b) => b.clone(),
                        None => return Ok(0),
                    };

                    let results = bridge
                        .db_search(&query, k as usize)
                        .await
                        .map_err(|e| wasmtime::Error::msg(format!("db_search error: {}", e)))?;

                    // Serialize results to JSON
                    let serialized = serde_json::to_vec(&results)
                        .map_err(|e| wasmtime::Error::msg(format!("serialization error: {}", e)))?;

                    // We store the last result in the state for later retrieval or write to guest if buffer exists
                    // For now, we return length and the guest must call 'db_get_response'
                    Ok(serialized.len() as i32)
                })
            },
        )
        .map_err(|e| MemFuseError::Sandbox(e.to_string()))?;

    // db_insert(key_ptr, key_len, val_ptr, val_len) -> i32
    linker
        .func_wrap4_async(
            "env",
            "db_insert",
            |mut caller: Caller<'_, SandboxState>,
             key_ptr: i32,
             key_len: i32,
             val_ptr: i32,
             val_len: i32| {
                Box::new(async move {
                    let memory = match caller.get_export("memory") {
                        Some(m) => m
                            .into_memory()
                            .ok_or_else(|| wasmtime::Error::msg("failed to get memory export"))?,
                        None => return Err(wasmtime::Error::msg("memory not exported")),
                    };

                    let mut key = vec![0u8; key_len as usize];
                    memory.read(&caller, key_ptr as usize, &mut key)?;

                    let mut val = vec![0u8; val_len as usize];
                    memory.read(&caller, val_ptr as usize, &mut val)?;

                    let bridge = match &caller.data().bridge {
                        Some(b) => b.clone(),
                        None => return Ok(0),
                    };

                    bridge
                        .db_insert(&key, &val)
                        .await
                        .map_err(|e| wasmtime::Error::msg(format!("db_insert error: {}", e)))?;

                    Ok(0)
                })
            },
        )
        .map_err(|e| MemFuseError::Sandbox(e.to_string()))?;

    // db_get(key_ptr, key_len) -> i32
    linker
        .func_wrap2_async(
            "env",
            "db_get",
            |mut caller: Caller<'_, SandboxState>, key_ptr: i32, key_len: i32| {
                Box::new(async move {
                    let _memory = match caller.get_export("memory") {
                        Some(m) => m
                            .into_memory()
                            .ok_or_else(|| wasmtime::Error::msg("failed to get memory export"))?,
                        None => return Err(wasmtime::Error::msg("memory not exported")),
                    };

                    let mut key = vec![0u8; key_len as usize];
                    _memory.read(&caller, key_ptr as usize, &mut key)?;

                    let bridge = match &caller.data().bridge {
                        Some(b) => b.clone(),
                        None => return Ok(0),
                    };

                    let val = bridge
                        .db_get(&key)
                        .await
                        .map_err(|e| wasmtime::Error::msg(format!("db_get error: {}", e)))?;

                    Ok(val.map(|v| v.len()).unwrap_or(0) as i32)
                })
            },
        )
        .map_err(|e| MemFuseError::Sandbox(e.to_string()))?;

    Ok(())
}
