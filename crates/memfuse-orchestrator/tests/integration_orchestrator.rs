// AGENT:07
// ANCHOR:INTEGRATION STATUS:DONE
use memfuse_orchestrator::StateGraph;

#[test]
fn test_integration_stategraph_agent_interaction() {
    let mut graph = StateGraph::new();

    // Add nodes
    graph.add_node("input", "Initial user query");
    graph.add_node("agent", "Primary LLM Agent");
    graph.add_node("tools", "WASM Tool execution");
    graph.add_node("output", "Final response");

    // Add edges
    graph.add_edge("input", "agent", None);
    graph.add_edge("agent", "tools", Some("needs_tool"));
    graph.add_edge("tools", "agent", Some("tool_result"));
    graph.add_edge("agent", "output", Some("complete"));

    assert_eq!(graph.nodes.len(), 4);
    assert_eq!(graph.edges.len(), 4);

    // Execute placeholder
    graph.run_workflow("Run integration test workflow");
}

#[test]
fn test_stategraph_node_integrity() {
    let mut graph = StateGraph::new();
    graph.add_node("A", "Node A");

    let node_a = graph.nodes.get("A").expect("Node A should exist");
    assert_eq!(node_a.id, "A");
    assert_eq!(node_a.description, "Node A");
}
