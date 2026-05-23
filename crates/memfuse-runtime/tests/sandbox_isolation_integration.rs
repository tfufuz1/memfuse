use memfuse_runtime::{AgentRuntime, WasmSandbox};

#[tokio::test]
async fn test_sandbox_config_defaults() {
    let _sandbox = WasmSandbox::new(64);
}

#[tokio::test]
async fn test_sandbox_isolation_and_execution() {
    let sandbox = WasmSandbox::new(64);

    // Test execution with placeholder bytes (simulating WASM payload)
    let wasm_bytes = b"\x00asm\x01\x00\x00\x00";
    let result = sandbox
        .execute_isolated(wasm_bytes, &memfuse_core::TokenBudget::new(100, 0))
        .await
        .expect("execution failed");

    // In the current placeholder implementation, it returns an empty vector
    assert!(result.is_empty());
}

#[tokio::test]
async fn test_sandbox_multiple_instances() {
    let s1 = WasmSandbox::new(64);
    let s2 = WasmSandbox::new(64);

    let res1 = s1
        .execute_isolated(b"", &memfuse_core::TokenBudget::new(100, 0))
        .await
        .unwrap();
    let res2 = s2
        .execute_isolated(b"", &memfuse_core::TokenBudget::new(100, 0))
        .await
        .unwrap();

    assert_eq!(res1, res2);
}
