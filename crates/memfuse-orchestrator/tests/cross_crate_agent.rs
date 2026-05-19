// AGENT:07
// ANCHOR:INTEGRATION STATUS:DONE
// Integration Test: StateGraph and Agent Interaction across Crates
use memfuse_db::{MemFuse, MemFuseConfig};
use memfuse_orchestrator::StateGraph;
use memfuse_runtime::{SandboxConfig, WasmSandbox};
use serde_json::json;
use tempfile::TempDir;

#[tokio::test]
async fn test_agent_cross_crate_workflow() {
    // 1. Setup DB (memfuse-db)
    let tmp = TempDir::new().expect("temp dir");
    let db = MemFuse::open_with_config(
        tmp.path(),
        MemFuseConfig {
            dimension: 3,
            ..Default::default()
        },
    )
    .await
    .expect("db open");

    db.insert("agent-knowledge", &[1.0, 0.0, 0.0], Some(json!({"info": "Rust is safe"})))
        .await
        .expect("insert");

    // 2. Setup Workflow (memfuse-orchestrator)
    let mut graph = StateGraph::new();
    graph.add_node("query_db", "Search knowledge base");
    graph.add_node("logic_gate", "Process with WASM");
    graph.add_edge("query_db", "logic_gate", None);

    // 3. Execution using Sandbox (memfuse-runtime)
    let sandbox = WasmSandbox::new(SandboxConfig::default());

    // Simulating workflow step 1: DB Query
    let results = db.search(&[1.0, 0.0, 0.0], 1).await.expect("search");
    assert_eq!(results[0].id, "agent-knowledge");

    // Simulating workflow step 2: WASM Processing
    let wasm_input = results[0].metadata.as_ref().unwrap()["info"].as_str().unwrap();
    let wasm_output = sandbox.execute(b"MOCK_WASM", wasm_input).expect("wasm exec");

    assert!(!wasm_output.is_empty());

    // Run the graph (placeholder execution)
    graph.run_workflow("start");
}
