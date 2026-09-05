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

#[tokio::test]
async fn test_decision_node_condition_branching() {
    let tmp = tempfile::TempDir::new().expect("temp dir");
    let config = memfuse_db::MemFuseConfig {
        dimension: 4,
        max_elements: 1000,
        distance_metric: memfuse_db::DistanceMetric::Cosine,
        ..Default::default()
    };
    let db = std::sync::Arc::new(
        memfuse_db::MemFuse::open_with_config(tmp.path(), config)
            .await
            .expect("open db"),
    );

    let storage = db.inner_storage();
    let engine = memfuse_agent::OrchestratorEngine::new(storage);

    let mut graph = StateGraph::new();
    graph.try_add_node("start", "Start node", NodeType::Start, None).unwrap();
    graph.try_add_node("decision", "Branch decision", NodeType::Decision, None).unwrap();
    graph.try_add_node("high_prio_path", "High Prio Path", NodeType::End, None).unwrap();
    graph.try_add_node("low_prio_path", "Low Prio Path", NodeType::End, None).unwrap();

    graph.try_add_edge("start", "decision", None, 1).unwrap();

    // Edge A: High priority (10), condition requires "flag == false"
    graph.try_add_edge("decision", "high_prio_path", Some("flag == false"), 10).unwrap();
    // Edge B: Low priority (1), condition requires "flag == true"
    graph.try_add_edge("decision", "low_prio_path", Some("flag == true"), 1).unwrap();

    let state_col = db.collection("agent-state").await.expect("state col");
    let budget = memfuse_core::TokenBudget::new(1000, 0);
    let mut ctx = memfuse_agent::AgentContext::try_new("task-decision-1", "start", db.clone(), state_col, budget).unwrap();

    // Set memory flag to true
    ctx.memory.insert("flag".to_string(), serde_json::json!(true));

    engine.run(&mut ctx, &graph).await.expect("run engine");

    // The decision node must follow low_prio_path because flag == true is satisfied,
    // whereas high_prio_path (priority 10) had flag == false (unsatisfied).
    assert_eq!(ctx.current_node, "low_prio_path");
    assert_eq!(ctx.status, memfuse_agent::AgentStatus::Completed);
}
