// FILE-CONTEXT Header (Format v3)
// ZWECK: Test token budget concurrency and RMW race condition in memfuse-agent workflow steps.
// INVARIANTEN: Verifies sequential vs concurrent TokenBudget consumption limits.
// STAND: TS:2026-08-31T00:00:00Z (SESSION: 8a7c2f1e)

use memfuse_agent::step::{AgentTool, StepResult};
use memfuse_agent::{AgentContext, NodeType, OrchestratorEngine, StateGraph};
use memfuse_core::{BoxFuture, Result, TokenBudget};
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

impl AgentTool for HeavyTokenTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn execute<'a>(
        &'a self,
        _ctx: &'a AgentContext,
        _input: serde_json::Value,
    ) -> BoxFuture<'a, Result<StepResult>> {
        Box::pin(async move {
            Ok(StepResult {
                node_id: self.name.clone(),
                output: serde_json::json!({"status": "done"}),
                tokens_consumed: self.tokens,
                next_edge: None,
            })
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
    let (mut engine, _db, mut ctx, _tmp) = setup_env(60).await?;

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

    // First step consumes 60/60 tokens -> 0 left.
    // Second step pre-check detects budget exhaustion before tool execution.
    let res = engine.run(&mut ctx, &graph).await;
    assert!(
        res.is_err(),
        "Expected engine.run to fail on budget exhaustion, got Ok"
    );
    match &res {
        Err(err) => {
            println!("Got error from engine.run: {:?}", err);
            assert!(
                err.to_string().contains("Token budget exhausted"),
                "Expected 'Token budget exhausted' in error, got: {:?}",
                err
            );
        }
        Ok(_) => unreachable!(),
    }

    Ok(())
}

#[tokio::test]
async fn test_estimated_cost_pre_execution_check() -> Result<()> {
    struct ExpensiveTool {
        executed: Arc<std::sync::atomic::AtomicBool>,
    }

    impl AgentTool for ExpensiveTool {
        fn name(&self) -> &str {
            "expensive_tool"
        }

        fn estimated_cost(&self, _input: &serde_json::Value) -> usize {
            100
        }

        fn execute<'a>(
            &'a self,
            _ctx: &'a AgentContext,
            _input: serde_json::Value,
        ) -> BoxFuture<'a, Result<StepResult>> {
            Box::pin(async move {
                self.executed
                    .store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(StepResult {
                    node_id: "expensive_tool".to_string(),
                    output: serde_json::json!({"status": "done"}),
                    tokens_consumed: 100,
                    next_edge: None,
                })
            })
        }
    }

    let (mut engine, _db, mut ctx, _tmp) = setup_env(50).await?; // Budget 50 < Estimated Cost 100
    let executed_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));

    let mut graph = StateGraph::new();
    graph.try_add_node("start", "Start Node", NodeType::Start, None)?;
    graph.try_add_node("step_1", "Step 1", NodeType::Task, Some("expensive_tool"))?;
    graph.try_add_node("end", "End Node", NodeType::End, None)?;

    graph.try_add_edge("start", "step_1", None, 1)?;
    graph.try_add_edge("step_1", "end", None, 1)?;

    engine.try_register_tool(Box::new(ExpensiveTool {
        executed: executed_flag.clone(),
    }))?;

    let res = engine.run(&mut ctx, &graph).await;
    assert!(res.is_err());
    // Verify tool execute was NEVER invoked due to strict pre-check
    assert!(
        !executed_flag.load(std::sync::atomic::Ordering::SeqCst),
        "Tool side effect was executed despite insufficient budget!"
    );

    Ok(())
}

struct SideEffectTool {
    cost: usize,
    side_effects: Arc<std::sync::atomic::AtomicUsize>,
}

impl AgentTool for SideEffectTool {
    fn name(&self) -> &str {
        "side_effect_tool"
    }

    fn estimated_cost(&self, _input: &serde_json::Value) -> usize {
        self.cost
    }

    fn execute<'a>(
        &'a self,
        _ctx: &'a AgentContext,
        _input: serde_json::Value,
    ) -> BoxFuture<'a, Result<StepResult>> {
        Box::pin(async move {
            // Execute side effect
            tokio::task::yield_now().await;
            self.side_effects
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(StepResult {
                node_id: "side_effect_tool".to_string(),
                output: serde_json::json!({"status": "done"}),
                tokens_consumed: self.cost,
                next_edge: None,
            })
        })
    }
}

#[tokio::test]
async fn test_atomic_budget_reservation_concurrency_stress() -> Result<()> {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let tool_cost = 30;
    let max_budget = 100;

    // Run 100 stress iterations to verify atomic budget reservation before tool.execute under high race conditions
    for _iteration in 0..100 {
        let budget = Arc::new(Mutex::new(TokenBudget::new(max_budget, 0)));
        let side_effect_counter = Arc::new(AtomicUsize::new(0));

        let num_workers = 50;
        let mut handles = Vec::with_capacity(num_workers);

        for _ in 0..num_workers {
            let budget_ref = budget.clone();
            let tool = SideEffectTool {
                cost: tool_cost,
                side_effects: side_effect_counter.clone(),
            };

            handles.push(tokio::spawn(async move {
                // ATOMIC CHECK-AND-RESERVE via TokenBudget::try_reserve before tool execution
                let reserved = {
                    let mut guard = budget_ref.lock().await;
                    let est = tool.estimated_cost(&serde_json::Value::Null);
                    guard.try_reserve(est).is_ok()
                };

                if reserved {
                    let dummy_tmp = tempfile::tempdir().unwrap();
                    let config = MemFuseConfig::default();
                    let db = Arc::new(
                        MemFuse::open_with_config(dummy_tmp.path(), config)
                            .await
                            .unwrap(),
                    );
                    let state_col = db.collection("dummy_col").await.unwrap();
                    let ctx = AgentContext::try_new(
                        "task-1",
                        "start",
                        db,
                        state_col,
                        TokenBudget::new(max_budget, 0),
                    )
                    .unwrap();

                    tool.execute(&ctx, serde_json::Value::Null).await.unwrap();
                    Ok::<(), ()>(())
                } else {
                    Err::<(), ()>(())
                }
            }));
        }

        for h in handles {
            let _ = h.await;
        }

        let executed_side_effects = side_effect_counter.load(Ordering::SeqCst);
        let final_guard = budget.lock().await;
        let total_consumed = final_guard.consumed();

        // With max_budget=100 and tool_cost=30, at most 3 side-effects can ever execute (3 * 30 = 90 <= 100)
        assert!(
            executed_side_effects <= 3,
            "Side effects ({}) exceeded maximum allowable (3) under race condition!",
            executed_side_effects
        );
        assert!(
            total_consumed <= max_budget,
            "Total consumed tokens ({}) exceeded max budget ({})!",
            total_consumed,
            max_budget
        );
    }

    Ok(())
}
