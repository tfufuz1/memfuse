use memfuse_agent::{NodeType, StateGraph};

#[tokio::test]
async fn test_stategraph_construction() {
    let mut graph = StateGraph::new();
    graph.add_node(
        "research",
        "Researching...",
        NodeType::Task,
        Some("search_tool"),
    );
    graph.add_node(
        "code",
        "Generating code...",
        NodeType::Task,
        Some("code_gen_tool"),
    );

    graph.add_edge("research", "code", Some("research_complete"), 1);

    assert_eq!(graph.nodes.len(), 2);
    assert_eq!(graph.edges.len(), 1);
}

#[test]
fn test_stategraph_boundary_validation() {
    let mut graph = StateGraph::new();

    // Empty Node ID validation
    let node_res = graph.try_add_node("", "Invalid empty node", NodeType::Task, None);
    assert!(node_res.is_err());
    if let Err(memfuse_core::MemFuseError::InvalidInput(msg)) = node_res {
        assert!(msg.contains("StateGraph node id must not be empty"));
    } else {
        panic!("Expected InvalidInput error");
    }

    // Empty Node description validation
    let desc_res = graph.try_add_node("node_x", "", NodeType::Task, None);
    assert!(desc_res.is_err());
    if let Err(memfuse_core::MemFuseError::InvalidInput(msg)) = desc_res {
        assert!(msg.contains("StateGraph node description must not be empty"));
    } else {
        panic!("Expected InvalidInput error");
    }

    // Empty Edge endpoint validation
    let edge_res1 = graph.try_add_edge("", "node_b", None, 1);
    assert!(edge_res1.is_err());

    let edge_res2 = graph.try_add_edge("node_a", "   ", None, 1);
    assert!(edge_res2.is_err());

    // Valid node & edge addition
    assert!(graph
        .try_add_node("node_a", "Valid node A", NodeType::Start, None)
        .is_ok());
    assert!(graph
        .try_add_node("node_b", "Valid node B", NodeType::End, None)
        .is_ok());
    assert!(graph.try_add_edge("node_a", "node_b", None, 1).is_ok());

    assert_eq!(graph.nodes.len(), 2);
    assert_eq!(graph.edges.len(), 1);
}

#[test]
fn test_background_event_boundary_validation() {
    use memfuse_agent::event_source::BackgroundEvent;

    let empty_source_res = BackgroundEvent::try_new(serde_json::json!({"test": 1}), "", 10);
    assert!(empty_source_res.is_err());

    let whitespace_source_res = BackgroundEvent::try_new(serde_json::json!({"test": 1}), "   ", 10);
    assert!(whitespace_source_res.is_err());

    let valid_evt = BackgroundEvent::try_new(serde_json::json!({"test": 1}), "source_a", 10);
    assert!(valid_evt.is_ok());
    assert_eq!(valid_evt.unwrap().source, "source_a");
}

#[tokio::test]
async fn test_agent_context_boundary_validation() {
    use memfuse_agent::context::AgentContext;
    use memfuse_core::TokenBudget;
    use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
    use std::sync::Arc;
    use tempfile::TempDir;

    let tmp = TempDir::new().expect("failed to create temp dir");
    let config = MemFuseConfig {
        dimension: 3,
        max_elements: 1000,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };
    let db = Arc::new(
        MemFuse::open_with_config(tmp.path(), config)
            .await
            .expect("failed to open db"),
    );
    let state_col = db.collection("agent_state").await.expect("col failed");

    // Empty task_id validation
    let err_task = AgentContext::try_new(
        "",
        "start_node",
        db.clone(),
        state_col.clone(),
        TokenBudget::new(100, 0),
    );
    assert!(err_task.is_err());

    // Empty start_node validation
    let err_node = AgentContext::try_new(
        "task_1",
        "",
        db.clone(),
        state_col.clone(),
        TokenBudget::new(100, 0),
    );
    assert!(err_node.is_err());

    // Valid AgentContext creation
    let valid_ctx = AgentContext::try_new(
        "task_1",
        "start_node",
        db,
        state_col,
        TokenBudget::new(100, 0),
    );
    assert!(valid_ctx.is_ok());
}
