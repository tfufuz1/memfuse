use memfuse_agent::context::{AgentContext, AgentStatus};
use memfuse_agent::engine::OrchestratorEngine;
use memfuse_agent::graph::{NodeType, StateGraph};
use memfuse_core::TokenBudget;
use memfuse_db::MemFuse;
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn test_atomic_final_state_checkpoint() -> memfuse_core::Result<()> {
    let tmp = TempDir::new().unwrap();
    let config = memfuse_db::MemFuseConfig {
        dimension: 1,
        ..Default::default()
    };
    let db = Arc::new(MemFuse::open_with_config(tmp.path(), config).await?);

    // Create or get a state collection
    let collection = db.collection("state").await?;

    let storage = db.inner_storage();
    let engine = OrchestratorEngine::new(storage);

    let mut graph = StateGraph::new();
    graph.try_add_node("start", "Start", NodeType::Start, None).unwrap();
    graph.try_add_node("end", "End", NodeType::End, None).unwrap();
    graph.try_add_edge("start", "end", None, 1).unwrap();

    let mut ctx = AgentContext::try_new(
        "test-task",
        "start",
        db.clone(),
        collection.clone(),
        TokenBudget::new(100, 0),
    )?;

    engine.run(&mut ctx, &graph).await?;

    assert_eq!(ctx.status, AgentStatus::Completed);

    // Verify that a checkpoint exists for the 'end' node
    let checkpoints = engine.checkpoint_store.list_checkpoints().await?;
    let end_checkpoint = checkpoints.iter().find(|c| c.name.contains(":node:end"));

    assert!(
        end_checkpoint.is_some(),
        "Checkpoint at 'end' node must exist"
    );

    // Verify final state is persisted
    let final_state = collection.get("task:test-task:final").await?;
    assert!(final_state.is_some(), "Final state must be persisted");

    Ok(())
}
