//! Token Budget Enforcement & Boundary Test Suite for `memfuse-agent`.
//!
//! Audits token budget consumption rules, post-check behavior, zero-budget initialization,
//! and exact budget boundary conditions.

use memfuse_agent::step::StepResult;
use memfuse_agent::{AgentContext, AgentTool, NodeType, OrchestratorEngine, StateGraph};
use memfuse_core::TokenBudget;
use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tempfile::TempDir;

struct BudgetTestTool {
    name: String,
    tokens: usize,
    exec_count: Arc<AtomicUsize>,
}

impl BudgetTestTool {
    fn new(name: &str, tokens: usize) -> (Self, Arc<AtomicUsize>) {
        let count = Arc::new(AtomicUsize::new(0));
        (
            Self {
                name: name.to_string(),
                tokens,
                exec_count: count.clone(),
            },
            count,
        )
    }
}

#[async_trait::async_trait]
impl AgentTool for BudgetTestTool {
    fn name(&self) -> &str {
        &self.name
    }

    async fn execute(
        &self,
        _ctx: &AgentContext,
        _input: serde_json::Value,
    ) -> memfuse_core::Result<StepResult> {
        self.exec_count.fetch_add(1, Ordering::SeqCst);
        Ok(StepResult {
            node_id: self.name.clone(),
            output: json!({"status": "executed"}),
            tokens_consumed: self.tokens,
            next_edge: None,
        })
    }
}

async fn setup_budget_env(
    task_id: &str,
    max_tokens: usize,
) -> (OrchestratorEngine, Arc<MemFuse>, AgentContext, TempDir) {
    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: 3,
        max_elements: 1_000,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };
    let db = Arc::new(
        MemFuse::open_with_config(tmp.path(), config)
            .await
            .expect("open db"),
    );

    let state_col = db.collection("agent-state").await.expect("collection");
    let ctx = AgentContext::try_new(
        task_id,
        "start",
        db.clone(),
        state_col,
        TokenBudget::new(max_tokens, 0),
    )
    .expect("AgentContext try_new");

    let engine = OrchestratorEngine::from_db(&db);
    (engine, db, ctx, tmp)
}

fn build_two_task_graph() -> StateGraph {
    let mut g = StateGraph::new();
    g.try_add_node("start", "Start", NodeType::Start, None)
        .unwrap();
    g.try_add_node("task1", "Task 1", NodeType::Task, Some("tool1"))
        .unwrap();
    g.try_add_node("task2", "Task 2", NodeType::Task, Some("tool2"))
        .unwrap();
    g.try_add_node("end", "End", NodeType::End, None).unwrap();

    g.try_add_edge("start", "task1", None, 1).unwrap();
    g.try_add_edge("task1", "task2", None, 1).unwrap();
    g.try_add_edge("task2", "end", None, 1).unwrap();
    g
}

// ─── 1. MID-WORKFLOW BUDGET EXHAUSTION ──────────────────────────────────────

#[tokio::test]
async fn test_budget_exhaustion_mid_workflow() {
    // Budget = 50 tokens.
    // task1 consumes 30 tokens -> remaining = 20 tokens.
    // task2 consumes 30 tokens -> remaining = 0 -> triggers error.
    let (mut engine, _db, mut ctx, _tmp) = setup_budget_env("task-budget-mid", 50).await;

    let (tool1, count1) = BudgetTestTool::new("tool1", 30);
    let (tool2, count2) = BudgetTestTool::new("tool2", 30);

    engine.try_register_tool(Box::new(tool1)).unwrap();
    engine.try_register_tool(Box::new(tool2)).unwrap();

    let graph = build_two_task_graph();

    let res = engine.run(&mut ctx, &graph).await;
    assert!(res.is_err(), "Expected budget exhaustion error");
    let err_str = res.unwrap_err().to_string();
    assert!(
        err_str.contains("Token budget exhausted"),
        "Unexpected error message: {err_str}"
    );

    assert_eq!(count1.load(Ordering::SeqCst), 1);
    assert_eq!(count2.load(Ordering::SeqCst), 1); // Post-check design: tool2 runs before check
    assert_eq!(ctx.status, memfuse_agent::AgentStatus::Failed);
}

// ─── 2. EXACT BUDGET EXHAUSTION ON LAST STEP ─────────────────────────────────

#[tokio::test]
async fn test_exact_budget_exhaustion() {
    // Budget = 50 tokens.
    // task1 consumes 25 tokens -> remaining = 25 tokens.
    // task2 consumes 25 tokens -> remaining = 0.
    let (mut engine, _db, mut ctx, _tmp) = setup_budget_env("task-budget-exact", 50).await;

    let (tool1, count1) = BudgetTestTool::new("tool1", 25);
    let (tool2, count2) = BudgetTestTool::new("tool2", 25);

    engine.try_register_tool(Box::new(tool1)).unwrap();
    engine.try_register_tool(Box::new(tool2)).unwrap();

    let graph = build_two_task_graph();

    let res = engine.run(&mut ctx, &graph).await;
    assert!(res.is_err(), "Exact exhaustion to 0 triggers error");
    assert_eq!(count1.load(Ordering::SeqCst), 1);
    assert_eq!(count2.load(Ordering::SeqCst), 1);
    assert_eq!(ctx.status, memfuse_agent::AgentStatus::Failed);
}

// ─── 3. ZERO INITIAL BUDGET (POST-CHECK DESIGN DEMONSTRATION) ───────────────

#[tokio::test]
async fn test_zero_initial_budget_post_check_behavior() {
    // Initial budget = 0 tokens.
    // Demonstrate post-check behavior: task1 tool executes once before budget check halts workflow.
    let (mut engine, _db, mut ctx, _tmp) = setup_budget_env("task-budget-zero", 0).await;

    let (tool1, count1) = BudgetTestTool::new("tool1", 10);
    let (tool2, count2) = BudgetTestTool::new("tool2", 10);

    engine.try_register_tool(Box::new(tool1)).unwrap();
    engine.try_register_tool(Box::new(tool2)).unwrap();

    let graph = build_two_task_graph();

    let res = engine.run(&mut ctx, &graph).await;
    assert!(res.is_err(), "Expected budget exhaustion error");
    assert_eq!(
        count1.load(Ordering::SeqCst),
        1,
        "Post-check design allows step1 execution before halting"
    );
    assert_eq!(
        count2.load(Ordering::SeqCst),
        0,
        "step2 must not be reached"
    );
    assert_eq!(ctx.status, memfuse_agent::AgentStatus::Failed);
}
