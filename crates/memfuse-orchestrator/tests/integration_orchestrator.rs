use memfuse_orchestrator::StateGraph;
use memfuse_db::MemFuse;
use tempfile::TempDir;

#[tokio::test]
async fn test_stategraph_agent_interaction() {
    let mut graph = StateGraph::new();

    graph.add_node("research", "Researches a topic using search tools");
    graph.add_node("summarize", "Summarizes the research findings");

    graph.add_edge("research", "summarize", None);

    // Verify graph structure
    assert_eq!(graph.nodes.len(), 2);
    assert_eq!(graph.edges.len(), 1);

    // Test workflow execution placeholder
    graph.run_workflow("Perform research on Rust lifetimes");
}

#[tokio::test]
async fn test_orchestrator_with_db_mock() {
    let tmp = TempDir::new().unwrap();
    let db = MemFuse::open(tmp.path()).await.unwrap();

    let mut graph = StateGraph::new();
    graph.add_node("db_query", "Queries MemFuse for relevant context");

    // Mock interaction: Orchestrator would typically use DB results to drive transitions
    db.insert("context-1", &[0.1; 1536], Some(serde_json::json!({"text": "context"}))).await.unwrap();

    let results = db.search(&[0.1; 1536], 1).await.unwrap();
    assert!(!results.is_empty());

    graph.run_workflow("query_start");
}
