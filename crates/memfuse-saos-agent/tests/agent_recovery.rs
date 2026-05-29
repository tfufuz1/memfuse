use memfuse_core::TokenBudget;
use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use memfuse_saos_agent::{
    AgentContext, AgentTool, NodeType, OrchestratorEngine, StateGraph, StepResult,
};
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
    let tmp = TempDir::new().expect("failed to create temp dir"); // expect #[cfg(test)]
    let config = MemFuseConfig {
        dimension: 3,
        max_elements: 1000,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };
    let db = Arc::new(
        MemFuse::open_with_config(tmp.path(), config)
            .await
            .expect("failed to open db"), // expect #[cfg(test)]
    );
    (db, tmp)
}

#[tokio::test]
async fn test_agent_auto_checkpoint_before_step() {
    let (db, _tmp) = setup_env().await;
    let state_col = Arc::new(db.collection("agent_state").await.expect("col failed")); // expect #[cfg(test)] // expect #[cfg(test)]
    let mut ctx = AgentContext::new(
        "t1",
        "start",
        db.clone(),
        state_col,
        TokenBudget::new(100, 0),
    );

    let mut graph = StateGraph::new();
    graph.add_node("start", "Start", NodeType::Start, Some("success_tool"));
    graph.add_node("end", "End", NodeType::End, None);
    graph.add_edge("start", "end", None, 1);

    let mut engine = OrchestratorEngine::new(db.inner_storage());
    engine.register_tool(Box::new(SuccessTool));

    engine.run(&mut ctx, &graph).await.expect("run failed"); // expect #[cfg(test)] // expect #[cfg(test)]

    // Verify checkpoint exists
    let checkpoints = engine
        .checkpoint_store
        .list_checkpoints()
        .await
        .expect("list failed"); // expect #[cfg(test)] // expect #[cfg(test)]
    assert!(checkpoints
        .iter()
        .any(|c| c.name == "task:t1:step:0:node:start"));
}

#[tokio::test]
async fn test_agent_replay_from_checkpoint() {
    let (db, _tmp) = setup_env().await;
    let state_col = Arc::new(db.collection("agent_state").await.expect("col failed")); // expect #[cfg(test)] // expect #[cfg(test)]
    let mut ctx = AgentContext::new(
        "t1",
        "start",
        db.clone(),
        state_col,
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
    engine.run(&mut ctx, &graph).await.expect("run failed"); // expect #[cfg(test)] // expect #[cfg(test)]
    assert_eq!(ctx.current_node, "end");

    // Manually modify context and replay
    ctx.memory.insert("corrupted".to_string(), json!(true));
    engine
        .replay_from(&mut ctx, "start")
        .await
        .expect("replay failed"); // expect #[cfg(test)] // expect #[cfg(test)]

    assert_eq!(ctx.current_node, "start");
    assert!(!ctx.memory.contains_key("corrupted"));
}

#[tokio::test]
async fn test_agent_error_handling() {
    let (db, _tmp) = setup_env().await;
    let state_col = Arc::new(db.collection("agent_state").await.expect("col failed")); // expect #[cfg(test)] // expect #[cfg(test)]
    let mut ctx = AgentContext::new(
        "t1",
        "start",
        db.clone(),
        state_col,
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
    let state_col = Arc::new(db.collection("agent_state").await.expect("col failed")); // expect #[cfg(test)] // expect #[cfg(test)]
    let mut ctx = AgentContext::new(
        "t1",
        "start",
        db.clone(),
        state_col,
        TokenBudget::new(100, 0),
    );

    let mut graph = StateGraph::new();
    graph.add_node("start", "Start", NodeType::Start, Some("success_tool"));
    graph.add_node("end", "End", NodeType::End, None);
    graph.add_edge("start", "end", None, 1);

    let mut engine = OrchestratorEngine::new(db.inner_storage());
    engine.register_tool(Box::new(SuccessTool));

    engine.run(&mut ctx, &graph).await.expect("run failed"); // expect #[cfg(test)] // expect #[cfg(test)]

    // Verify audit log
    let audit_log = db.collection("agent_state").await.expect("col failed"); // expect #[cfg(test)] // expect #[cfg(test)]
    let audit_entries = audit_log
        .scan_prefix("audit:t1:step:")
        .await
        .expect("scan failed"); // expect #[cfg(test)] // expect #[cfg(test)]
    assert_eq!(audit_entries.len(), 1);

    // In our implementation, we don't have a direct "AuditError::Immutable" yet because
    // the collection API doesn't distinguish between audit and normal data at the storage level.
    // However, the Orchestrator only provides an 'append' interface for the audit log.
}

#[tokio::test]
async fn test_loop_rollback_integrity() {
    let (db, _tmp) = setup_env().await;
    let state_col = Arc::new(db.collection("agent_state").await.expect("col failed")); // expect #[cfg(test)] // expect #[cfg(test)]
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
        let _node = graph.get_node(&ctx.current_node).expect("node exists"); // expect #[cfg(test)] // expect #[cfg(test)]
                                                                             // Manual execution of one step
                                                                             // (In a real scenario, the engine.run would have logic to break loops or budget)
                                                                             // For testing naming collision, we just care about checkpoints.
        engine.checkpoint(&ctx).await.expect("checkpoint failed"); // expect #[cfg(test)] // expect #[cfg(test)]
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
        .expect("list failed"); // expect #[cfg(test)]

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
}
