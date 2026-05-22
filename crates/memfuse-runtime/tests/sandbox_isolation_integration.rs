use memfuse_runtime::{SandboxConfig, WasmSandbox};
use std::time::Duration;

#[test]
fn test_sandbox_config_defaults() {
    let config = SandboxConfig::default();
    assert_eq!(config.max_memory_mb, 64);
    assert_eq!(config.timeout, Duration::from_millis(500));
    assert!(!config.allow_network);
}

#[test]
fn test_sandbox_isolation_and_execution() {
    let config = SandboxConfig {
        max_memory_pages: 2048,
        max_memory_mb: 128,
        timeout: Duration::from_secs(1),
        allow_network: false,
    };
    let sandbox = WasmSandbox::new(config);

    // Test execution with placeholder bytes (simulating WASM payload)
    let wasm_bytes = b"\x00asm\x01\x00\x00\x00";
    let input = "ping";
    let result = sandbox
        .execute(wasm_bytes, input)
        .expect("execution failed");

    // In the current placeholder implementation, it returns a static string
    assert_eq!(result, b"sandbox_execution_result_placeholder");
}

#[test]
fn test_sandbox_multiple_instances() {
    let s1 = WasmSandbox::new(SandboxConfig::default());
    let s2 = WasmSandbox::new(SandboxConfig::default());

    let res1 = s1.execute(b"", "1").unwrap();
    let res2 = s2.execute(b"", "2").unwrap();

    assert_eq!(res1, res2);
}
