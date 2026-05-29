use memfuse_core::{Result, TokenBudget};
use memfuse_sandbox::{AgentRuntime, SandboxConfig, WasmSandbox};

fn dummy_wasm() -> Vec<u8> {
    wat::parse_str(
        r#"
        (module
            (func (export "main"))
        )
    "#,
    )
    .unwrap() // unwrap #[cfg(test)]
}

#[tokio::test]
async fn test_sandbox_initialization() {
    let _sandbox = WasmSandbox::new(SandboxConfig::default()).unwrap(); // unwrap #[cfg(test)] // unwrap #[cfg(test)]
}

#[tokio::test]
async fn test_sandbox_execution_placeholder() -> Result<()> {
    let sandbox = WasmSandbox::new(SandboxConfig::default()).unwrap(); // unwrap #[cfg(test)] // unwrap #[cfg(test)]
    let wasm_bytes = dummy_wasm();
    let budget = TokenBudget::new(100, 0);

    let result = sandbox.execute_isolated(&wasm_bytes, &budget).await?;

    assert!(result.is_empty());
    Ok(())
}
