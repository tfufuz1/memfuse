// FILE-CONTEXT Header (Format v3)
// ZWECK: Test token budget concurrency and RMW race condition in memfuse-agent workflow steps.
// INVARIANTEN: Verifies sequential vs concurrent TokenBudget consumption limits.
// STAND: TS:2026-08-31T00:00:00Z (SESSION: 8a7c2f1e)

use memfuse_agent::step::{AgentTool, StepResult};
use memfuse_agent::{AgentContext, NodeType, OrchestratorEngine, StateGraph};
use memfuse_core::{MemFuseError, Result, TokenBudget};
use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::Mutex;

struct HeavyTokenTool {
    name: String,
    tokens: usize,
}

impl HeavyTokenTool {
    fn new(name: &str, tokens: usize) -> Self {
        Self {
            name: name.to_string(),
            tokens,
        }
    }
}

#[async_trait::async_trait]
impl AgentTool for HeavyTokenTool {
    fn name(&self) -> &str {
        &self.name
    }

    async fn execute(&self, _ctx: &AgentContext, _input: serde_json::Value) -> Result<StepResult> {
        Ok(StepResult {
            node_id: self.name.clone(),
            output: serde_json::json!({"status": "done"}),
            tokens_consumed: self.tokens,
            next_edge: None,
        })
    }
}

async fn setup_env(
    max_tokens: usize,
) -> Result<(OrchestratorEngine, Arc<MemFuse>, AgentContext, TempDir)> {
    let tmp_dir = tempfile::tempdir().unwrap();
    let config = MemFuseConfig {
        dimension: 128,
        max_elements: 1000,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };
    let db = Arc::new(MemFuse::open_with_config(tmp_dir.path(), config).await?);
    let state_col = db.collection("agent_state").await?;
    let engine = OrchestratorEngine::from_db(&db);
    let ctx = AgentContext::try_new(
        "task-race-1",
        "start",
        db.clone(),
        state_col,
        TokenBudget::new(max_tokens, 0),
    )?;
    Ok((engine, db, ctx, tmp_dir))
}

#[tokio::test]
async fn test_sequential_workflow_budget_check() -> Result<()> {
    let (mut engine, _db, mut ctx, _tmp) = setup_env(100).await?;

    let mut graph = StateGraph::new();
    graph.try_add_node("start", "Start Node", NodeType::Start, None)?;
    graph.try_add_node("step_1", "Step 1", NodeType::Task, Some("tool_1"))?;
    graph.try_add_node("step_2", "Step 2", NodeType::Task, Some("tool_2"))?;
    graph.try_add_node("end", "End Node", NodeType::End, None)?;

    graph.try_add_edge("start", "step_1", None, 1)?;
    graph.try_add_edge("step_1", "step_2", None, 1)?;
    graph.try_add_edge("step_2", "end", None, 1)?;

    engine.try_register_tool(Box::new(HeavyTokenTool::new("tool_1", 60)))?;
    engine.try_register_tool(Box::new(HeavyTokenTool::new("tool_2", 60)))?;

    // First step consumes 60/100 tokens -> 40 left.
    // Second step consumes 60 tokens -> total 120 -> budget exhausted (available == 0).
    let res = engine.run(&mut ctx, &graph).await;
    assert!(res.is_err());
    if let Err(MemFuseError::Internal(msg)) = res {
        assert!(msg.contains("Token budget exhausted"));
    } else {
        panic!("Expected Token budget exhausted error");
    }

    Ok(())
}

#[tokio::test]
async fn test_concurrent_budget_consumption_rmw_race() -> Result<()> {
    // Shared TokenBudget with 100 max tokens limit
    let budget = Arc::new(Mutex::new(TokenBudget::new(100, 0)));

    let mut handles = Vec::new();

    // Spawn 2 parallel worker tasks each trying to check budget and consume 80 tokens concurrently
    for _i in 0..2 {
        let budget_ref = budget.clone();
        handles.push(tokio::spawn(async move {
            let available = {
                let guard = budget_ref.lock().await;
                guard.available()
            };
            if available >= 50 {
                // Simulate async execution gap between read and write
                tokio::task::yield_now().await;
                let mut guard = budget_ref.lock().await;
                guard.consume(80);
                Ok::<usize, String>(80)
            } else {
                Err::<usize, String>("Insufficient budget".to_string())
            }
        }));
    }

    let mut success_count = 0;
    for h in handles {
        if let Ok(Ok(_tokens)) = h.await {
            success_count += 1;
        }
    }

    let final_guard = budget.lock().await;
    let total_consumed = final_guard.consumed();

    // Both parallel tasks read available (100) before either updated consumed, so both succeed and consume 160 tokens total
    assert_eq!(
        success_count, 2,
        "Both tasks succeeded due to RMW race condition"
    );
    assert_eq!(total_consumed, 160);
    assert!(
        total_consumed > 100,
        "RMW Race confirmed: total consumed exceeds budget"
    );

    Ok(())
}
