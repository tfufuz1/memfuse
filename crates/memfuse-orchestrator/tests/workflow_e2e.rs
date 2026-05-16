//! Integration tests for MemFuse Orchestrator.
//!
//! ANCHOR:TEST:ORCHESTRATOR-WORKFLOW-001 STATUS:READY AGENT:12

use memfuse_orchestrator::{StateGraph};

#[test]
fn test_stategraph_declaration() {
    let mut graph = StateGraph::new();
    graph.add_node("start", "Entry");
    graph.add_node("end", "Exit");
    graph.add_edge("start", "end", None);

    assert_eq!(graph.nodes.len(), 2);
    assert_eq!(graph.edges.len(), 1);
}

#[test]
fn test_workflow_execution_stub() {
    let mut graph = StateGraph::new();
    graph.add_node("task", "A single task node");
    graph.run_workflow("task");
}
