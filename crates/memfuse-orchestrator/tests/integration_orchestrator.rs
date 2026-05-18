//! Integration tests for StateGraph and Agent interactions.
// ANCHOR:INTEGRATION PRIO:2 STATUS:DONE AGENT:07 DATE:2026-05-21

use memfuse_orchestrator::{AgentNode, StateGraph};

#[test]
fn test_stategraph_construction() {
    let mut graph = StateGraph::new();

    graph.add_node("researcher", "Primary search agent");
    graph.add_node("writer", "Content generation agent");

    graph.add_edge("researcher", "writer", Some("found_content"));

    assert_eq!(graph.nodes.len(), 2);
    assert_eq!(graph.edges.len(), 1);

    assert!(graph.nodes.contains_key("researcher"));
    assert!(graph.nodes.contains_key("writer"));
}

#[test]
fn test_agent_node_properties() {
    let node = AgentNode {
        id: "test-id".to_string(),
        description: "test-desc".to_string(),
    };
    assert_eq!(node.id, "test-id");
    assert_eq!(node.description, "test-desc");
}

#[test]
fn test_stategraph_run_empty() {
    let graph = StateGraph::new();
    // Should not panic
    graph.run_workflow("start");
}
