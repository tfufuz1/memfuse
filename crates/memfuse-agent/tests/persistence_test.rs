use memfuse_agent::context::{AgentContext, AgentStatus};
use memfuse_agent::engine::OrchestratorEngine;
use memfuse_agent::graph::{NodeType, StateGraph};
use memfuse_agent::step::{AgentTool, StepResult};
use memfuse_core::TokenBudget;
use memfuse_db::{MemFuse, MemFuseConfig};
use std::sync::Arc;
use tempfile::TempDir;

struct IncrementTool;

#[async_trait::async_trait]
impl AgentTool for IncrementTool {
    fn name(&self) -> &str {
        "increment"
    }

    async fn execute(
        &self,
        _ctx: &AgentContext,
        input: serde_json::Value,
    ) -> memfuse_core::Result<StepResult> {
        let val = input.as_u64().unwrap_or(0);
        Ok(StepResult {
            node_id: "task_1".to_string(),
            output: serde_json::json!(val + 1),
            tokens_consumed: 10,
            next_edge: None,
        })
    }
}

#[tokio::test]
async fn test_agent_persistence_and_recovery() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path();

    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };

    let db = Arc::new(MemFuse::open_with_config(db_path, config).await.unwrap());
    let state_collection = db.collection("agent_state").await.unwrap();

    let mut graph = StateGraph::new();
    graph.add_node("start", "Start Node", NodeType::Start, None);
    graph.add_node(
        "task_1",
        "Increment Task",
        NodeType::Task,
        Some("increment"),
    );
    graph.add_node("end", "End Node", NodeType::End, None);

    graph.add_edge("start", "task_1", None, 1);
    graph.add_edge("task_1", "end", None, 1);

    let mut engine = OrchestratorEngine::new(db.inner_storage());
    engine.register_tool(Box::new(IncrementTool));

    let mut ctx = AgentContext::new(
        "test_task_123",
        "start",
        db.clone(),
        state_collection.clone(),
        TokenBudget::new(100, 0),
    );

    // Initial run
    engine.run(&mut ctx, &graph).await.expect("Run failed");

    assert_eq!(ctx.status, AgentStatus::Completed);
    assert_eq!(ctx.memory.get("last_output").unwrap().as_u64().unwrap(), 1);

    // Verify persistence in DB
    let final_doc = state_collection
        .get("task:test_task_123:final")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(final_doc.metadata.unwrap()["status"], "Completed");

    // Test Replay (Simulation of recovery)
    let db2 = Arc::new(
        MemFuse::open_with_config(
            db_path,
            MemFuseConfig {
                dimension: 4,
                ..Default::default()
            },
        )
        .await
        .unwrap(),
    );
    let state_collection2 = db2.collection("agent_state").await.unwrap();

    let mut ctx2 = AgentContext::new(
        "test_task_123",
        "task_1", // Start from task_1 for replay
        db2.clone(),
        state_collection2.clone(),
        TokenBudget::new(100, 0),
    );

    engine
        .replay_from(&mut ctx2, "task_1")
        .await
        .expect("Replay failed");

    // Deep assertions post-recovery
    assert_eq!(ctx2.current_node, "task_1");
    assert_eq!(ctx2.step_count, 1);

    // Continue execution post-recovery to completion
    let engine2 = OrchestratorEngine::new(db2.inner_storage());
    let mut engine2 = engine2;
    engine2.register_tool(Box::new(IncrementTool));
    engine2.run(&mut ctx2, &graph).await.expect("Resume run failed");

    assert_eq!(ctx2.status, AgentStatus::Completed);
    assert_eq!(ctx2.current_node, "end");
    assert_eq!(ctx2.memory.get("last_output").unwrap().as_u64().unwrap(), 1);

    // Audit trail verification on recovered DB
    let audit_log = memfuse_agent::audit::AuditLog::new(state_collection2);
    let audit_entries = audit_log.replay_task("test_task_123").await.expect("audit replay");
    assert!(!audit_entries.is_empty());
    for entry in &audit_entries {
        assert_eq!(entry.task_id, "test_task_123");
    }
}
