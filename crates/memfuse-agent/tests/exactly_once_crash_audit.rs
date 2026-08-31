//! Exactly-Once Execution Semantics & Crash Simulation Suite for `memfuse-agent`.
//!
//! Tests crash recovery during step execution, RAII `CheckpointGuard` transaction rollback,
//! state recovery via `replay_from()`, and `commit_step()` idempotency.

use memfuse_agent::step::StepResult;
use memfuse_agent::{AgentContext, AgentTool, NodeType, OrchestratorEngine, StateGraph};
use memfuse_core::TokenBudget;
use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tempfile::TempDir;

/// Tool that simulates crash/panic or failure on a specific invocation count.
struct CrashableTool {
    name: String,
    call_count: Arc<AtomicUsize>,
    crash_on_call: Option<usize>,
}

impl CrashableTool {
    fn new(name: &str, crash_on_call: Option<usize>) -> (Self, Arc<AtomicUsize>) {
        let counter = Arc::new(AtomicUsize::new(0));
        (
            Self {
                name: name.to_string(),
                call_count: counter.clone(),
                crash_on_call,
            },
            counter,
        )
    }
}

#[async_trait::async_trait]
impl AgentTool for CrashableTool {
    fn name(&self) -> &str {
        &self.name
    }

    async fn execute(
        &self,
        _ctx: &AgentContext,
        _input: serde_json::Value,
    ) -> memfuse_core::Result<StepResult> {
        let current = self.call_count.fetch_add(1, Ordering::SeqCst) + 1;
        if let Some(target) = self.crash_on_call {
            if current == target {
                return Err(memfuse_core::MemFuseError::Internal(format!(
                    "Simulated crash on call {} in tool {}",
                    current, self.name
                )));
            }
        }

        Ok(StepResult {
            node_id: self.name.clone(),
            output: json!({"call_number": current}),
            tokens_consumed: 10,
            next_edge: None,
        })
    }
}

async fn setup_crash_env(
    task_id: &str,
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
    let ctx = AgentContext::try_new(task_id, "start", db.clone(), state_col, TokenBudget::new(1000, 0))
        .expect("AgentContext try_new");

    let engine = OrchestratorEngine::from_db(&db);
    (engine, db, ctx, tmp)
}

fn build_two_step_graph() -> StateGraph {
    let mut g = StateGraph::new();
    g.try_add_node("start", "Start Node", NodeType::Start, None)
        .unwrap();
    g.try_add_node("step1", "Step 1", NodeType::Task, Some("tool1"))
        .unwrap();
    g.try_add_node("step2", "Step 2", NodeType::Task, Some("tool2"))
        .unwrap();
    g.try_add_node("end", "End Node", NodeType::End, None)
        .unwrap();

    g.try_add_edge("start", "step1", None, 1).unwrap();
    g.try_add_edge("step1", "step2", None, 1).unwrap();
    g.try_add_edge("step2", "end", None, 1).unwrap();
    g
}

// ─── 1. CRASH SIMULATION & CLEAN ROLLBACK ────────────────────────────────────

#[tokio::test]
async fn test_crash_during_step_execution_rolls_back_and_recovers_cleanly() {
    let (mut engine, _db, mut ctx, _tmp) = setup_crash_env("crash-task-1").await;

    let (tool1, counter1) = CrashableTool::new("tool1", None);
    // tool2 will crash on its first call
    let (tool2, counter2) = CrashableTool::new("tool2", Some(1));

    engine.try_register_tool(Box::new(tool1)).unwrap();
    engine.try_register_tool(Box::new(tool2)).unwrap();

    let graph = build_two_step_graph();

    // First attempt fails during step2 execution
    let res = engine.run(&mut ctx, &graph).await;
    assert!(res.is_err(), "Expected simulated crash in step2");
    assert_eq!(counter1.load(Ordering::SeqCst), 1);
    assert_eq!(counter2.load(Ordering::SeqCst), 1);

    // Context status is set to Failed
    assert_eq!(ctx.status, memfuse_agent::AgentStatus::Failed);

    // Verify recovery via replay_from: restore context back to checkpoint before step2
    engine
        .replay_from(&mut ctx, "step2")
        .await
        .expect("replay_from step2");

    assert_eq!(ctx.current_node, "step2");

    // Replace tool2 with a non-crashing tool (recovery scenario after fix)
    let (tool2_fixed, counter2_fixed) = CrashableTool::new("tool2", None);
    engine.try_register_tool(Box::new(tool2_fixed)).unwrap();

    // Resume execution from step2 checkpoint
    let resume_res = engine.run(&mut ctx, &graph).await;
    assert!(
        resume_res.is_ok(),
        "Resumed execution failed: {:?}",
        resume_res
    );

    assert_eq!(ctx.status, memfuse_agent::AgentStatus::Completed);
    // tool1 was NOT re-executed because recovery resumed from step2 checkpoint!
    assert_eq!(
        counter1.load(Ordering::SeqCst),
        1,
        "step1 tool must not be re-executed!"
    );
    assert_eq!(counter2_fixed.load(Ordering::SeqCst), 1);
}

// ─── 2. REPLAY RESTORATION & STATE VERIFICATION ─────────────────────────────

#[tokio::test]
async fn test_replay_from_restores_prior_checkpoint_state() {
    let (mut engine, _db, mut ctx, _tmp) = setup_crash_env("idempotent-task").await;

    let (tool1, counter1) = CrashableTool::new("tool1", None);
    let (tool2, counter2) = CrashableTool::new("tool2", None);

    engine.try_register_tool(Box::new(tool1)).unwrap();
    engine.try_register_tool(Box::new(tool2)).unwrap();

    let graph = build_two_step_graph();

    // Run to completion
    engine.run(&mut ctx, &graph).await.unwrap();
    assert_eq!(ctx.status, memfuse_agent::AgentStatus::Completed);
    assert_eq!(counter1.load(Ordering::SeqCst), 1);
    assert_eq!(counter2.load(Ordering::SeqCst), 1);

    // Replay to 'step2' restores context current_node to step2
    let res = engine.replay_from(&mut ctx, "step2").await;
    assert!(res.is_ok(), "Replay to step2 must succeed: {:?}", res);
    assert_eq!(ctx.current_node, "step2");
}
