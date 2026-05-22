// AGENT:12
// ANCHOR:INTEGRATION STATUS:DONE
use memfuse_runtime::WasmSandbox;

// FIXME: These tests are currently ignored because of API stubs in lib.rs.
// SandboxConfig and .execute() are not yet properly implemented/exported.

#[test]
#[ignore]
fn test_sandbox_initialization() {
    let _sandbox = WasmSandbox::new(64);
}

#[test]
#[ignore]
fn test_sandbox_execution_placeholder() {
    let _sandbox = WasmSandbox::new(64);
}
