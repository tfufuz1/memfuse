use memfuse_core::TokenBudget;
use memfuse_sandbox::{AgentRuntime, SandboxConfig, WasmSandbox};

fn dummy_wasm() -> Vec<u8> {
    wat::parse_str(
        r#"
        (module
            (func (export "main"))
        )
    "#,
    )
    .unwrap() // unwrap allowed (AGENT:08)
}

#[tokio::test]
async fn test_sandbox_initialization() {
    let _sandbox = WasmSandbox::new(SandboxConfig::default()).unwrap(); // unwrap allowed (AGENT:08)
}

#[tokio::test]
async fn test_sandbox_isolation_and_execution() {
    let sandbox = WasmSandbox::new(SandboxConfig::default()).unwrap(); // unwrap allowed (AGENT:08)

    let wasm_bytes = dummy_wasm();
    let budget = TokenBudget::new(100, 0);
    let result = sandbox
        .execute_isolated(&wasm_bytes, &budget)
        .await
        .expect("execution failed"); // unwrap allowed (AGENT:08)

    assert!(result.is_empty());
}

#[tokio::test]
async fn test_sandbox_multiple_instances() {
    let s1 = WasmSandbox::new(SandboxConfig::default()).unwrap(); // unwrap allowed (AGENT:08)
    let s2 = WasmSandbox::new(SandboxConfig::default()).unwrap(); // unwrap allowed (AGENT:08)
    let budget = TokenBudget::new(10, 0);

    let wasm_bytes = dummy_wasm();
    let res1 = s1.execute_isolated(&wasm_bytes, &budget).await.unwrap(); // unwrap allowed (AGENT:08)
    let res2 = s2.execute_isolated(&wasm_bytes, &budget).await.unwrap(); // unwrap allowed (AGENT:08)

    assert_eq!(res1, res2);
}
