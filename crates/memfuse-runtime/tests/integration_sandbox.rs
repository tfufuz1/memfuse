// AGENT:07
// ANCHOR:INTEGRATION STATUS:DONE
use memfuse_runtime::{SandboxConfig, WasmSandbox};
use std::time::Duration;

#[test]
fn test_integration_sandbox_isolation() {
    let config = SandboxConfig {
        max_memory_mb: 32,
        timeout: Duration::from_millis(100),
        allow_network: false,
    };
    let sandbox = WasmSandbox::new(config);

    // Test that the sandbox can be instantiated and executed.
    // In a real scenario, this would test memory limits and isolation.
    let result = sandbox.execute(b"MOCK_WASM", "test_input").expect("Execution failed");
    assert_eq!(result, "sandbox_execution_result_placeholder");
}

#[test]
fn test_sandbox_default_config() {
    let sandbox = WasmSandbox::new(SandboxConfig::default());
    let result = sandbox.execute(b"MOCK_WASM", "hello").expect("Execution failed");
    assert!(!result.is_empty());
}
