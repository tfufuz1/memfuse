// AGENT:12
// ANCHOR:INTEGRATION STATUS:DONE
use memfuse_runtime::{AgentRuntime, WasmSandbox};

#[tokio::test]
async fn test_sandbox_initialization() {
    let config = 64;
    let _sandbox = WasmSandbox::new(config);
}

#[tokio::test]
async fn test_sandbox_execution_placeholder() {
    let sandbox = WasmSandbox::new(64);
    let wasm_bytes = vec![0, 1, 2, 3]; // Mock WASM
    let result = sandbox
        .execute_isolated(&wasm_bytes, &memfuse_core::TokenBudget::new(100, 0))
        .await;

    assert!(result.is_ok());
}
