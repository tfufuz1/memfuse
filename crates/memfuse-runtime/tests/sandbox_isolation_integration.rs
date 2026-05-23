use memfuse_core::TokenBudget;
use memfuse_runtime::{AgentRuntime, WasmSandbox};

#[tokio::test]
async fn test_sandbox_initialization() {
    let _sandbox = WasmSandbox::new(64);
}

#[tokio::test]
async fn test_sandbox_isolation_and_execution() {
    let sandbox = WasmSandbox::new(128);

    let wasm_bytes = b"\x00asm\x01\x00\x00\x00";
    let budget = TokenBudget::new(100, 0);
    let result = sandbox
        .execute_isolated(wasm_bytes, &budget)
        .await
        .expect("execution failed");

    assert!(result.is_empty());
}

#[tokio::test]
async fn test_sandbox_multiple_instances() {
    let s1 = WasmSandbox::new(64);
    let s2 = WasmSandbox::new(64);
    let budget = TokenBudget::new(10, 0);

    let res1 = s1.execute_isolated(b"", &budget).await.unwrap();
    let res2 = s2.execute_isolated(b"", &budget).await.unwrap();

    assert_eq!(res1, res2);
}
