//! Integration Test for StateGraph and Agent interactions.
// ANCHOR:INTEGRATION STATUS:DONE AGENT:07 DATE:2026-05-18

use memfuse_orchestrator::StateGraph;
use memfuse_runtime::{SandboxConfig, WasmSandbox};

#[tokio::test]
async fn test_orchestrator_runtime_interaction() {
    let mut graph = StateGraph::new();

    // Define a simple 2-node workflow
    graph.add_node("ingest", "Ingest and Clean Data");
    graph.add_node("analyze", "Analyze with WASM Tool");
    graph.add_edge("ingest", "analyze", None);

    // Verify graph structure
    assert_eq!(graph.nodes.len(), 2);
    assert_eq!(graph.edges.len(), 1);

    // Simulate execution context
    let _sandbox = WasmSandbox::new(SandboxConfig::default());

    // In a real scenario, the graph executor would use the sandbox
    graph.run_workflow("ingest");
}

#[tokio::test]
async fn test_stategraph_cycle_detection_placeholder() {
    let mut graph = StateGraph::new();
    graph.add_node("A", "Node A");
    graph.add_node("B", "Node B");
    graph.add_edge("A", "B", None);
    graph.add_edge("B", "A", Some("loop"));

    assert_eq!(graph.edges.len(), 2);
    // run_workflow should handle or detect cycles
    graph.run_workflow("A");
}
