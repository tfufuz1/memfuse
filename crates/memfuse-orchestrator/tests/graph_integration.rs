use memfuse_orchestrator::{GraphNode, StateGraph, WorkflowEdge};

#[test]
fn test_stategraph_complex_workflow() {
    let mut graph = StateGraph::new();

    // Define nodes
    graph.nodes.push(GraphNode {
        name: "ingress".to_string(),
        executable_identifier: "ingress_tool".to_string(),
    });
    graph.nodes.push(GraphNode {
        name: "analyze".to_string(),
        executable_identifier: "analyze_tool".to_string(),
    });
    graph.nodes.push(GraphNode {
        name: "store".to_string(),
        executable_identifier: "store_tool".to_string(),
    });
    graph.nodes.push(GraphNode {
        name: "notify".to_string(),
        executable_identifier: "notify_tool".to_string(),
    });

    // Define edges with conditions
    graph.edges.push(WorkflowEdge {
        from: "ingress".to_string(),
        to: "analyze".to_string(),
        condition_evaluator: None,
    });
    graph.edges.push(WorkflowEdge {
        from: "analyze".to_string(),
        to: "store".to_string(),
        condition_evaluator: Some("is_valid".to_string()),
    });
    graph.edges.push(WorkflowEdge {
        from: "analyze".to_string(),
        to: "notify".to_string(),
        condition_evaluator: Some("is_invalid".to_string()),
    });
    graph.edges.push(WorkflowEdge {
        from: "store".to_string(),
        to: "notify".to_string(),
        condition_evaluator: None,
    });

    assert_eq!(graph.nodes.len(), 4);
    assert_eq!(graph.edges.len(), 4);

    // Check specific nodes
    assert!(graph.nodes.iter().any(|n| n.name == "ingress"));

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
