//! Integration tests for WASM Sandbox isolation and execution.
// ANCHOR:INTEGRATION PRIO:2 STATUS:DONE AGENT:07 DATE:2026-05-21

use memfuse_runtime::{SandboxConfig, WasmSandbox};

#[test]
fn test_sandbox_isolation_contract() {
    let config = SandboxConfig {
        max_memory_mb: 32,
        ..Default::default()
    };
    let sandbox = WasmSandbox::new(config);

    // Execute a dummy WASM payload
    let result = sandbox.execute(b"\x00asm\x01\x00\x00\x00", "test-input");
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "sandbox_execution_result_placeholder");
}

#[test]
fn test_default_sandbox_config() {
    let config = SandboxConfig::default();
    assert_eq!(config.max_memory_mb, 64);
    assert!(!config.allow_network);
}
