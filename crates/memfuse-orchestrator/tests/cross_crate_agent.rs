use memfuse_db::{MemFuse, MemFuseConfig};
use memfuse_orchestrator::StateGraph;
use tempfile::TempDir;

#[tokio::test]
async fn test_cross_crate_agent_graph() {
    let tmp = TempDir::new().unwrap();
    let db = MemFuse::open_with_config(
        tmp.path(),
        MemFuseConfig {
            dimension: 4,
            ..Default::default()
        },
    )
    .await
    .unwrap();

    // Setup Agent Graph
    let mut graph = StateGraph::new();
    graph.add_node("search", "Search documents");
    graph.add_node("summarize", "Summarize results");
    graph.add_edge("search", "summarize", None);

    // Simulate search
    db.insert("d1", &[1.0, 0.0, 0.0, 0.0], None).await.unwrap();
    let results = db.search(&[1.0, 0.0, 0.0, 0.0], 1).await.unwrap();
    assert!(!results.is_empty());

    // In a real orchestrator, these results would be passed to 'summarize'
    graph.run_workflow("initial context");
}
