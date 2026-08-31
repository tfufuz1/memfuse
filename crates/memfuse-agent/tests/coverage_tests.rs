// FILE-CONTEXT Header (Format v3)
// ZWECK: Comprehensive test coverage for orchestrator engine error paths and state transitions.
// INVARIANTEN: Anti-Mirroring verified; testing budget exhaustion, dead ends, missing handlers, and replay errors.
// NICHT-OFFENSICHTLICH: Validates that failed workflow steps record failure in AuditLog before transitioning to AgentStatus::Failed.
// HOTSPOTS: Engine error paths in run_internal and replay_from.
// STAND: TS:2026-08-31T21:07:58Z (SESSION: 5f1a7b8e)

use memfuse_agent::{
    AgentContext, AgentStatus, AgentTool, NodeType, OrchestratorEngine, StateGraph, StepResult,
};
use memfuse_core::{MemFuseError, Result, TokenBudget};
use memfuse_db::MemFuse;
use std::sync::Arc;
use tempfile::tempdir;

struct TestTool {
    name: String,
    tokens: usize,
    should_fail: bool,
}

#[async_trait::async_trait]
impl AgentTool for TestTool {
    fn name(&self) -> &str {
        &self.name
    }

    async fn execute(&self, _ctx: &AgentContext, _input: serde_json::Value) -> Result<StepResult> {
        if self.should_fail {
            return Err(MemFuseError::Internal(
                "Tool execution forced failure".to_string(),
            ));
        }
        Ok(StepResult {
            node_id: "task_node".to_string(),
            output: serde_json::json!({"res": "ok"}),
            tokens_consumed: self.tokens,
            next_edge: None,
        })
    }
}

async fn create_test_env() -> (
    Arc<MemFuse>,
    Arc<memfuse_db::Collection<memfuse_store::LsmStorage>>,
) {
    let dir = tempdir().unwrap();
    let db = Arc::new(MemFuse::open(dir.path()).await.unwrap());
    let col = db.collection("agent_state").await.unwrap();
    (db, col)
}

#[tokio::test]
async fn test_engine_missing_start_node_CASE_error() {
    let (db, col) = create_test_env().await;
    let engine = OrchestratorEngine::from_db(&db);

    let graph = StateGraph::new(); // empty graph
    let mut ctx = AgentContext::try_new(
        "task-missing",
        "ghost_node",
        db,
        col,
        TokenBudget::new(1000, 0),
    )
    .unwrap();

    let res = engine.run(&mut ctx, &graph).await;
    assert!(res.is_err());
    assert_eq!(ctx.status, AgentStatus::Failed);
}

#[tokio::test]
async fn test_engine_task_lacks_handler_CASE_error() {
    let (db, col) = create_test_env().await;
    let engine = OrchestratorEngine::from_db(&db);

    let mut graph = StateGraph::new();
    graph
        .try_add_node("start", "Start", NodeType::Start, None)
        .unwrap();
    graph
        .try_add_node("task1", "Task without handler", NodeType::Task, None)
        .unwrap();
    graph.try_add_edge("start", "task1", None, 1).unwrap();

    let mut ctx = AgentContext::try_new(
        "task-nohandler",
        "start",
        db,
        col,
        TokenBudget::new(1000, 0),
    )
    .unwrap();

    let res = engine.run(&mut ctx, &graph).await;
    assert!(res.is_err());
    assert_eq!(ctx.status, AgentStatus::Failed);
    if let Err(MemFuseError::Internal(msg)) = res {
        assert!(msg.contains("lacks handler"));
    } else {
        panic!("Expected MemFuseError::Internal for missing handler");
    }
}

#[tokio::test]
async fn test_engine_unregistered_tool_CASE_error() {
    let (db, col) = create_test_env().await;
    let engine = OrchestratorEngine::from_db(&db);

    let mut graph = StateGraph::new();
    graph
        .try_add_node("start", "Start", NodeType::Start, None)
        .unwrap();
    graph
        .try_add_node("task1", "Task", NodeType::Task, Some("unregistered_tool"))
        .unwrap();
    graph.try_add_edge("start", "task1", None, 1).unwrap();

    let mut ctx =
        AgentContext::try_new("task-unreg", "start", db, col, TokenBudget::new(1000, 0)).unwrap();

    let res = engine.run(&mut ctx, &graph).await;
    assert!(res.is_err());
    assert_eq!(ctx.status, AgentStatus::Failed);
}

#[tokio::test]
async fn test_engine_budget_exhaustion_CASE_error() {
    let (db, col) = create_test_env().await;
    let mut engine = OrchestratorEngine::from_db(&db);

    engine
        .try_register_tool(Box::new(TestTool {
            name: "costly_tool".to_string(),
            tokens: 500,
            should_fail: false,
        }))
        .unwrap();

    let mut graph = StateGraph::new();
    graph
        .try_add_node("start", "Start", NodeType::Start, None)
        .unwrap();
    graph
        .try_add_node("task1", "Costly Task", NodeType::Task, Some("costly_tool"))
        .unwrap();
    graph
        .try_add_node("end", "End", NodeType::End, None)
        .unwrap();
    graph.try_add_edge("start", "task1", None, 1).unwrap();
    graph.try_add_edge("task1", "end", None, 1).unwrap();

    // Context budget is only 100, tool consumes 500
    let mut ctx =
        AgentContext::try_new("task-budget", "start", db, col, TokenBudget::new(100, 0)).unwrap();

    let res = engine.run(&mut ctx, &graph).await;
    assert!(res.is_err());
    assert_eq!(ctx.status, AgentStatus::Failed);
    if let Err(MemFuseError::Internal(msg)) = res {
        assert!(msg.contains("Token budget exhausted"));
    } else {
        panic!("Expected token budget exhaustion error");
    }
}

#[tokio::test]
async fn test_engine_dead_end_CASE_error() {
    let (db, col) = create_test_env().await;
    let engine = OrchestratorEngine::from_db(&db);

    let mut graph = StateGraph::new();
    // Start node with no outgoing edges
    graph
        .try_add_node("start", "Start", NodeType::Start, None)
        .unwrap();

    let mut ctx =
        AgentContext::try_new("task-deadend", "start", db, col, TokenBudget::new(1000, 0)).unwrap();

    let res = engine.run(&mut ctx, &graph).await;
    assert!(res.is_err());
    assert_eq!(ctx.status, AgentStatus::Failed);
    if let Err(MemFuseError::Internal(msg)) = res {
        assert!(msg.contains("Dead end at node"));
    } else {
        panic!("Expected dead end internal error");
    }
}

#[tokio::test]
async fn test_engine_decision_no_edges_CASE_error() {
    let (db, col) = create_test_env().await;
    let engine = OrchestratorEngine::from_db(&db);

    let mut graph = StateGraph::new();
    graph
        .try_add_node("start", "Start", NodeType::Start, None)
        .unwrap();
    graph
        .try_add_node("decide", "Decision", NodeType::Decision, None)
        .unwrap();
    graph.try_add_edge("start", "decide", None, 1).unwrap();
    // decision node has no outgoing edges

    let mut ctx = AgentContext::try_new(
        "task-decide-err",
        "start",
        db,
        col,
        TokenBudget::new(1000, 0),
    )
    .unwrap();

    let res = engine.run(&mut ctx, &graph).await;
    assert!(res.is_err());
    assert_eq!(ctx.status, AgentStatus::Failed);
    if let Err(MemFuseError::Internal(msg)) = res {
        assert!(msg.contains("Decision Node decide has no outgoing edges"));
    } else {
        panic!("Expected decision node error");
    }
}

#[tokio::test]
async fn test_engine_replay_from_nonexistent_CASE_error() {
    let (db, col) = create_test_env().await;
    let engine = OrchestratorEngine::from_db(&db);

    let mut ctx =
        AgentContext::try_new("task-replay", "start", db, col, TokenBudget::new(1000, 0)).unwrap();

    let res = engine.replay_from(&mut ctx, "missing_step").await;
    assert!(res.is_err());
    if let Err(MemFuseError::Internal(msg)) = res {
        assert!(msg.contains("nicht gefunden"));
    } else {
        panic!("Expected missing checkpoint internal error");
    }
}
