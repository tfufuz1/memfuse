//! E2E tests for MemFuse Orchestrator.

use memfuse_orchestrator::StateGraph;

#[test]
fn test_stategraph_building_e2e() {
    let mut graph = StateGraph::new();

    // 1. Add nodes
    graph.add_node("research", "Researches the topic using tools");
    graph.add_node("writer", "Writes a summary based on research");
    graph.add_node("reviewer", "Reviews the summary");

    // 2. Add edges
    graph.add_edge("research", "writer", None);
    graph.add_edge("writer", "reviewer", Some("needs_review"));
    graph.add_edge("reviewer", "writer", Some("rejected"));
    graph.add_edge("reviewer", "end", Some("approved"));

    // 3. Verify structure
    assert_eq!(graph.nodes.len(), 3);
    assert_eq!(graph.edges.len(), 4);

    assert!(graph.nodes.contains_key("research"));
    assert_eq!(graph.nodes.get("research").unwrap().description, "Researches the topic using tools");

    // 4. Run workflow (placeholder execution)
    graph.run_workflow("research");
}
