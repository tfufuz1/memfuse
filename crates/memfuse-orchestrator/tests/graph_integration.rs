use memfuse_orchestrator::StateGraph;

#[test]
fn test_stategraph_complex_workflow() {
    let mut graph = StateGraph::new();

    // Define nodes
    graph.add_node("ingress", "Data Ingress");
    graph.add_node("analyze", "Analyze Metadata");
    graph.add_node("store", "Store in MemFuse");
    graph.add_node("notify", "Notify Agent");

    // Define edges with conditions
    graph.add_edge("ingress", "analyze", None);
    graph.add_edge("analyze", "store", Some("is_valid"));
    graph.add_edge("analyze", "notify", Some("is_invalid"));
    graph.add_edge("store", "notify", None);

    assert_eq!(graph.nodes.len(), 4);
    assert_eq!(graph.edges.len(), 4);

    // Check specific nodes
    assert!(graph.nodes.contains_key("ingress"));
    assert_eq!(
        graph.nodes.get("ingress").unwrap().description,
        "Data Ingress"
    );

    // Check specific edges
    let edge_analyze_store = graph
        .edges
        .iter()
        .find(|e| e.from == "analyze" && e.to == "store")
        .unwrap();
    assert_eq!(
        edge_analyze_store.condition_evaluator,
        Some("is_valid".to_string())
    );
}

#[test]
fn test_stategraph_run_lifecycle() {
    let mut graph = StateGraph::new();
    graph.add_node("start", "Start node");
    graph.run_workflow("start");
    // run_workflow is currently a placeholder, so we just verify it doesn't panic
}
