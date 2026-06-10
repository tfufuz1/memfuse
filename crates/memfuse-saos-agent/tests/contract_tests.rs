//! Contract tests for WP-5.3 Acceptance Criteria.
//!
//! AC-1: Checkpoint before every step
//! AC-2: Replay from checkpoint
//! AC-3: Immutable audit log
//! Bonus: Token budget exhaustion

use memfuse_core::traits::StorageEngine;
use memfuse_core::TokenBudget;
use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use memfuse_saos_agent::audit::AuditLog;
use memfuse_saos_agent::step::StepResult;
use memfuse_saos_agent::{AgentContext, AgentTool, NodeType, OrchestratorEngine, StateGraph};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;

/// Tool that consumes a configurable number of tokens per call.
struct TokenTool {
    name: String,
    tokens: usize,
}

impl TokenTool {
    fn new(name: &str, tokens: usize) -> Self {
        Self {
            name: name.to_string(),
            tokens,
        }
    }
}

#[async_trait::async_trait]
impl AgentTool for TokenTool {
    fn name(&self) -> &str {
        &self.name
    }

    async fn execute(
        &self,
        _ctx: &AgentContext,
        _input: serde_json::Value,
    ) -> memfuse_core::Result<StepResult> {
        Ok(StepResult {
            node_id: self.name.clone(),
            output: json!({"tool": self.name, "status": "ok"}),
            tokens_consumed: self.tokens,
            next_edge: None,
        })
    }
}

/// Creates a test environment: engine + db + state_collection.
async fn setup(budget: TokenBudget) -> (OrchestratorEngine, Arc<MemFuse>, AgentContext, TempDir) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_test_writer()
        .try_init();

    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: 3,
        max_elements: 10_000,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };
    let db = Arc::new(
        MemFuse::open_with_config(tmp.path(), config)
            .await
            .expect("open db"),
    );

    let state_col = Arc::new(db.collection("agent-state").await.expect("collection"));
    let ctx = AgentContext::new("test-task", "start", db.clone(), state_col, budget);

    let storage = db.inner_storage();
    let engine = OrchestratorEngine::new(storage);

    (engine, db, ctx, tmp)
}

/// Build a linear graph: start → step_a → step_b → end
fn linear_graph() -> StateGraph {
    let mut g = StateGraph::new();
    g.add_node("start", "Begin", NodeType::Start, Some("tool_a"));
    g.add_node("step_a", "Step A", NodeType::Task, Some("tool_a"));
    g.add_node("step_b", "Step B", NodeType::Task, Some("tool_b"));
    g.add_node("end", "End", NodeType::End, None);

    g.add_edge("start", "step_a", None, 1);
    g.add_edge("step_a", "step_b", None, 1);
    g.add_edge("step_b", "end", None, 1);
    g
}

// ─── AC-1: Checkpoint before every step ───────────────────────────────

#[tokio::test]
async fn test_agent_auto_checkpoint_before_step() {
    let (mut engine, _db, mut ctx, _tmp) = setup(TokenBudget::new(1000, 0)).await;
    engine.register_tool(Box::new(TokenTool::new("tool_a", 5)));
    engine.register_tool(Box::new(TokenTool::new("tool_b", 5)));

    let graph = linear_graph();
    engine.run(&mut ctx, &graph).await.expect("run");

    // Verify checkpoints were created before each step via the checkpoint_store
    let cp_start = engine
        .checkpoint_store
        .get_checkpoint("task:test-task:step:0:node:start")
        .await
        .expect("get");
    assert!(cp_start.is_some(), "Checkpoint before 'start' must exist");

    let cp_a = engine
        .checkpoint_store
        .get_checkpoint("task:test-task:step:1:node:step_a")
        .await
        .expect("get");
    assert!(cp_a.is_some(), "Checkpoint before 'step_a' must exist");

    let cp_b = engine
        .checkpoint_store
        .get_checkpoint("task:test-task:step:2:node:step_b")
        .await
        .expect("get");
    assert!(cp_b.is_some(), "Checkpoint before 'step_b' must exist");

    let cp_end = engine
        .checkpoint_store
        .get_checkpoint("task:test-task:step:3:node:end")
        .await
        .expect("get");
    assert!(cp_end.is_some(), "Checkpoint at 'end' must exist");
}

// ─── AC-2: Replay from checkpoint ─────────────────────────────────────

#[tokio::test]
async fn test_agent_replay_from_checkpoint() {
    let (mut engine, _db, mut ctx, _tmp) = setup(TokenBudget::new(1000, 0)).await;
    engine.register_tool(Box::new(TokenTool::new("tool_a", 5)));
    engine.register_tool(Box::new(TokenTool::new("tool_b", 5)));

    let graph = linear_graph();

    // Run the workflow to completion
    engine.run(&mut ctx, &graph).await.expect("run");
    assert_eq!(ctx.current_node, "end");

    // Now replay from step_a — context should be restored
    engine
        .replay_from(&mut ctx, "step_a")
        .await
        .expect("replay");
    assert_eq!(ctx.current_node, "step_a");
}

// ─── AC-3: Immutable audit log ────────────────────────────────────────

#[tokio::test]
async fn test_agent_audit_log_immutable() {
    let (mut engine, _db, mut ctx, _tmp) = setup(TokenBudget::new(1000, 0)).await;
    engine.register_tool(Box::new(TokenTool::new("tool_a", 5)));
    engine.register_tool(Box::new(TokenTool::new("tool_b", 5)));

    let graph = linear_graph();
    engine.run(&mut ctx, &graph).await.expect("run");
    ctx.db.inner_storage().flush().await.expect("flush");

    // Replay the audit log and verify entries
    let audit = AuditLog::new(ctx.state_collection.clone());
    let entries = audit.replay_task("test-task").await.expect("replay");

    // 3 steps executed: start, step_a, step_b (end doesn't execute a tool)
    assert_eq!(
        entries.len(),
        3,
        "Expected 3 audit entries, got {}",
        entries.len()
    );

    // All entries must belong to our task
    for entry in &entries {
        assert_eq!(entry.task_id, "test-task");
    }

    // Entries must be ordered by step_count
    for (i, entry) in entries.iter().enumerate() {
        assert_eq!(entry.step_count, i as u64);
    }
}

// ─── Token budget exhaustion ──────────────────────────────────────────

#[tokio::test]
async fn test_token_budget_exhaustion() {
    // Budget of 10 max, 0 reserve → each tool consumes 6 → second step should fail
    let (mut engine, _db, mut ctx, _tmp) = setup(TokenBudget::new(10, 0)).await;
    engine.register_tool(Box::new(TokenTool::new("tool_a", 6)));
    engine.register_tool(Box::new(TokenTool::new("tool_b", 6)));

    let graph = linear_graph();
    let result = engine.run(&mut ctx, &graph).await;

    assert!(result.is_err(), "Expected budget exhaustion error");
    let err_msg = format!("{}", result.expect_err("error"));
    assert!(
        err_msg.contains("Token budget exhausted"),
        "Error should mention budget exhaustion, got: {}",
        err_msg
    );
}
