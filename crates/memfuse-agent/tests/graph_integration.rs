use memfuse_agent::{NodeType, StateGraph};

#[test]
fn test_stategraph_complex_workflow() {
    let mut graph = StateGraph::new();

    // Define nodes
    graph
        .try_add_node("ingress", "Ingress", NodeType::Start, Some("ingress_tool"))
        .unwrap();
    graph
        .try_add_node("analyze", "Analyze", NodeType::Task, Some("analyze_tool"))
        .unwrap();
    graph
        .try_add_node("store", "Store", NodeType::Task, Some("store_tool"))
        .unwrap();
    graph
        .try_add_node("notify", "Notify", NodeType::End, Some("notify_tool"))
        .unwrap();

    // Define edges with conditions
    graph.try_add_edge("ingress", "analyze", None, 1).unwrap();
    graph
        .try_add_edge("analyze", "store", Some("is_valid"), 2)
        .unwrap();
    graph
        .try_add_edge("analyze", "notify", Some("is_invalid"), 1)
        .unwrap();
    graph.try_add_edge("store", "notify", None, 1).unwrap();

    assert_eq!(graph.nodes.len(), 4);
    assert_eq!(graph.edges.len(), 4);

    // Check specific nodes
    assert!(graph.nodes.contains_key("ingress"));

    // Check specific edges
    let edge_analyze_store = graph
        .edges
        .iter()
        .find(|e| e.from == "analyze" && e.to == "store")
        .expect("edge not found");
    assert_eq!(edge_analyze_store.condition, Some("is_valid".to_string()));
}
