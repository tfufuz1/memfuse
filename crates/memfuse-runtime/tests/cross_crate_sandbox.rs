use memfuse_db::{MemFuse, MemFuseConfig};
use memfuse_runtime::{SandboxConfig, WasmSandbox};
use serde_json::json;
use tempfile::TempDir;

#[tokio::test]
async fn test_cross_crate_sandbox_data_flow() {
    let tmp = TempDir::new().unwrap();
    let db = MemFuse::open_with_config(
        tmp.path(),
        MemFuseConfig {
            dimension: 4,
            ..Default::default()
        },
    )
    .await
    .unwrap();

    // 1. Store data in DB
    db.insert("doc-1", &[1.0, 0.0, 0.0, 0.0], Some(json!({"content": "important data"})))
        .await
        .unwrap();

    // 2. Retrieve data
    let doc = db.get("doc-1").await.unwrap().unwrap();
    let input_for_sandbox = doc.metadata.unwrap()["content"].as_str().unwrap().to_string();

    // 3. Pass data to Sandbox
    let sandbox = WasmSandbox::new(SandboxConfig::default());
    let result = sandbox.execute(b"MOCK_WASM", &input_for_sandbox).unwrap();

    assert_eq!(result, "sandbox_execution_result_placeholder");
}
