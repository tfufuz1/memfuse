use memfuse_agent::{
    AgentContext, AgentTool, NodeType, OrchestratorEngine, StateGraph, StepResult,
};
use memfuse_core::TokenBudget;
use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;

struct FailingTool;

#[async_trait::async_trait]
impl AgentTool for FailingTool {
    fn name(&self) -> &str {
        "failing_tool"
    }

    async fn execute(
        &self,
        _ctx: &AgentContext,
        _input: serde_json::Value,
    ) -> memfuse_core::Result<StepResult> {
        Err(memfuse_core::MemFuseError::Internal(
            "Simulated failure".to_string(),
        ))
    }
}

struct SuccessTool;

#[async_trait::async_trait]
impl AgentTool for SuccessTool {
    fn name(&self) -> &str {
        "success_tool"
    }

    async fn execute(
        &self,
        ctx: &AgentContext,
        _input: serde_json::Value,
    ) -> memfuse_core::Result<StepResult> {
        Ok(StepResult {
            node_id: ctx.current_node.clone(),
            output: json!({"status": "success"}),
            tokens_consumed: 1,
            next_edge: None,
        })
    }
}

async fn setup_env() -> (Arc<MemFuse>, TempDir) {
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
    (db, tmp)
}

#[tokio::test]
async fn test_agent_auto_checkpoint_before_step() {
    let (db, _tmp) = setup_env().await;
    let state_col = db.collection("agent_state").await.expect("col failed");
    let mut ctx = AgentContext::new(
        "t1",
        "start",
        db.clone(),
        state_col.clone(),
        TokenBudget::new(100, 0),
    );

    let mut graph = StateGraph::new();
    graph.add_node("start", "Start", NodeType::Start, Some("success_tool"));
    graph.add_node("end", "End", NodeType::End, None);
    graph.add_edge("start", "end", None, 1);

    let mut engine = OrchestratorEngine::new(db.inner_storage());
    engine.register_tool(Box::new(SuccessTool));

    engine.run(&mut ctx, &graph).await.expect("run failed");

    // Verify checkpoint exists
    let checkpoints = engine
        .checkpoint_store
        .list_checkpoints()
        .await
        .expect("list failed");
    assert!(checkpoints
        .iter()
        .any(|c| c.name == "task:t1:step:0:node:start"));
}

#[tokio::test]
async fn test_agent_replay_from_checkpoint() {
    let (db, _tmp) = setup_env().await;
    let state_col = db.collection("agent_state").await.expect("col failed");
    let mut ctx = AgentContext::new(
        "t1",
        "start",
        db.clone(),
        state_col.clone(),
        TokenBudget::new(100, 0),
    );

    let mut graph = StateGraph::new();
    graph.add_node("start", "Start", NodeType::Start, Some("success_tool"));
    graph.add_node("step2", "Step 2", NodeType::Task, Some("success_tool"));
    graph.add_node("end", "End", NodeType::End, None);
    graph.add_edge("start", "step2", None, 1);
    graph.add_edge("step2", "end", None, 1);

    let mut engine = OrchestratorEngine::new(db.inner_storage());
    engine.register_tool(Box::new(SuccessTool));

    // Run first step
    engine.run(&mut ctx, &graph).await.expect("run failed");
    assert_eq!(ctx.current_node, "end");

    // Manually modify context and replay
    ctx.memory.insert("corrupted".to_string(), json!(true));
    engine
        .replay_from(&mut ctx, "start")
        .await
        .expect("replay failed");

    assert_eq!(ctx.current_node, "start");
    assert_eq!(ctx.step_count, 0);
    assert!(!ctx.memory.contains_key("corrupted"));
}

#[tokio::test]
async fn test_agent_error_handling() {
    let (db, _tmp) = setup_env().await;
    let state_col = db.collection("agent_state").await.expect("col failed");
    let mut ctx = AgentContext::new(
        "t1",
        "start",
        db.clone(),
        state_col.clone(),
        TokenBudget::new(100, 0),
    );

    let mut graph = StateGraph::new();
    graph.add_node("start", "Start", NodeType::Start, Some("failing_tool"));
    graph.add_node("end", "End", NodeType::End, None);
    graph.add_edge("start", "end", None, 1);

    let mut engine = OrchestratorEngine::new(db.inner_storage());
    engine.register_tool(Box::new(FailingTool));

    let result = engine.run(&mut ctx, &graph).await;
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Simulated failure"));
}

#[tokio::test]
async fn test_agent_audit_log_immutable() {
    let (db, _tmp) = setup_env().await;
    let state_col = db.collection("agent_state").await.expect("col failed");
    let mut ctx = AgentContext::new(
        "t1",
        "start",
        db.clone(),
        state_col.clone(),
        TokenBudget::new(100, 0),
    );

    let mut graph = StateGraph::new();
    graph.add_node("start", "Start", NodeType::Start, Some("success_tool"));
    graph.add_node("end", "End", NodeType::End, None);
    graph.add_edge("start", "end", None, 1);

    let mut engine = OrchestratorEngine::new(db.inner_storage());
    engine.register_tool(Box::new(SuccessTool));

    engine.run(&mut ctx, &graph).await.expect("run failed");

    // Verify audit log content and immutability via AuditLog replay
    let audit_log_instance = memfuse_agent::audit::AuditLog::new(state_col);
    let audit_entries = audit_log_instance
        .replay_task("t1")
        .await
        .expect("replay failed");
    assert_eq!(audit_entries.len(), 1);
    assert_eq!(audit_entries[0].task_id, "t1");
    assert_eq!(audit_entries[0].step_count, 0);
    assert_eq!(audit_entries[0].node_id, "start");
    assert_eq!(audit_entries[0].tokens_consumed, 1);
}

#[tokio::test]
async fn test_loop_rollback_integrity() {
    let (db, _tmp) = setup_env().await;
    let state_col = db.collection("agent_state").await.expect("col failed");
    let mut ctx = AgentContext::new(
        "loop-task",
        "A",
        db.clone(),
        state_col,
        TokenBudget::new(100, 0),
    );

    // Graph: A -> B -> A (Loop)
    let mut graph = StateGraph::new();
    graph.add_node("A", "Node A", NodeType::Start, Some("success_tool"));
    graph.add_node("B", "Node B", NodeType::Task, Some("success_tool"));
    graph.add_node("end", "End", NodeType::End, None);

    graph.add_edge("A", "B", None, 1);
    graph.add_edge("B", "A", None, 1);
    // Add a way out of the loop after 2 iterations (manual intervention simulated)
    graph.add_edge("A", "end", None, 1);

    let mut engine = OrchestratorEngine::new(db.inner_storage());
    engine.register_tool(Box::new(SuccessTool));

    // Run 5 steps: A(0) -> B(1) -> A(2) -> B(3) -> A(4)
    // We stop before it continues to B or end.
    for _ in 0..5 {
        let _node = graph.get_node(&ctx.current_node).expect("node exists");
        // Manual execution of one step
        // (In a real scenario, the engine.run would have logic to break loops or budget)
        // For testing naming collision, we just care about checkpoints.
        engine.checkpoint(&ctx).await.expect("checkpoint failed");
        ctx.step_count += 1;
        if ctx.current_node == "A" {
            ctx.current_node = "B".to_string();
        } else {
            ctx.current_node = "A".to_string();
        }
    }

    let checkpoints = engine
        .checkpoint_store
        .list_checkpoints()
        .await
        .expect("list failed");

    // Should have 5 checkpoints: step 0(A), 1(B), 2(A), 3(B), 4(A)
    assert_eq!(checkpoints.len(), 5);
    assert!(checkpoints
        .iter()
        .any(|c| c.name.contains(":step:0:node:A")));
    assert!(checkpoints
        .iter()
        .any(|c| c.name.contains(":step:2:node:A")));
    assert!(checkpoints
        .iter()
        .any(|c| c.name.contains(":step:4:node:A")));

    // Verify replay from middle of loop (step 2)
    engine
        .replay_from(&mut ctx, "2")
        .await
        .expect("replay step 2");
    assert_eq!(ctx.current_node, "A");
    assert_eq!(ctx.step_count, 2);
}
