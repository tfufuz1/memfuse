//! Integration tests for MemFuse Orchestrator.
//!
//! ANCHOR:TEST:ORCHESTRATOR-WORKFLOW-001 STATUS:READY AGENT:12
//! This suite verifies the declaration and execution of agent workflows
//! using the StateGraph abstraction.

use memfuse_orchestrator::{StateGraph};

#[test]
fn test_stategraph_declaration() {
    let mut graph = StateGraph::new();

    // 1. Add Nodes
    graph.add_node("start", "Initial entry point");
    graph.add_node("research", "Agent specializing in data retrieval");
    graph.add_node("coding", "Agent specializing in Rust implementation");
    graph.add_node("end", "Terminal node");

    // 2. Add Edges with conditions
    graph.add_edge("start", "research", None);
    graph.add_edge("research", "coding", Some("data_found"));
    graph.add_edge("research", "start", Some("more_info_needed"));
    graph.add_edge("coding", "end", None);

    // 3. Verify Graph structure
    assert_eq!(graph.nodes.len(), 4);
    assert_eq!(graph.edges.len(), 4);

    assert!(graph.nodes.contains_key("research"));

    let research_node = &graph.nodes["research"];
    assert_eq!(research_node.id, "research");
}

#[test]
fn test_workflow_execution_stub() {
    let mut graph = StateGraph::new();
    graph.add_node("task", "A single task node");
    graph.add_node("done", "Completion");
    graph.add_edge("task", "done", None);

    // This should not panic, even if execution logic is still a stub
    graph.run_workflow("task");
}
