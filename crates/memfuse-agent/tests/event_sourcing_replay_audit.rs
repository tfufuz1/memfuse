//! Event Sourcing & Replay Determinism Verification Test Suite for `memfuse-agent`.
//!
//! Verifies bit-for-bit state reconstruction from checkpoints and deterministic audit log replaying.

use memfuse_agent::audit::AuditLog;
use memfuse_agent::step::StepResult;
use memfuse_agent::{AgentContext, AgentTool, NodeType, OrchestratorEngine, StateGraph};
use memfuse_core::TokenBudget;
use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tempfile::TempDir;

struct DeterministicTool {
    name: String,
    multiplier: u64,
    call_count: Arc<AtomicUsize>,
}

impl DeterministicTool {
    fn new(name: &str, multiplier: u64) -> (Self, Arc<AtomicUsize>) {
        let count = Arc::new(AtomicUsize::new(0));
        (
            Self {
                name: name.to_string(),
                multiplier,
                call_count: count.clone(),
            },
            count,
        )
    }
}

#[async_trait::async_trait]
impl AgentTool for DeterministicTool {
    fn name(&self) -> &str {
        &self.name
    }

    async fn execute(
        &self,
        _ctx: &AgentContext,
        input: serde_json::Value,
    ) -> memfuse_core::Result<StepResult> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        let prev_val = input.get("acc").and_then(|v| v.as_u64()).unwrap_or(0);
        let new_val = prev_val + self.multiplier;

        Ok(StepResult {
            node_id: self.name.clone(),
            output: json!({"acc": new_val, "step_tool": self.name}),
            tokens_consumed: 12,
            next_edge: None,
        })
    }
}

async fn setup_replay_env(
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
    let ctx = AgentContext::try_new(
        task_id,
        "start",
        db.clone(),
        state_col,
        TokenBudget::new(1000, 0),
    )
    .expect("AgentContext try_new");

    let engine = OrchestratorEngine::from_db(&db);
    (engine, db, ctx, tmp)
}

fn build_pipeline_graph() -> StateGraph {
    let mut g = StateGraph::new();
    g.try_add_node("start", "Start Node", NodeType::Start, None)
        .unwrap();
    g.try_add_node("stage1", "Stage 1", NodeType::Task, Some("t1"))
        .unwrap();
    g.try_add_node("stage2", "Stage 2", NodeType::Task, Some("t2"))
        .unwrap();
    g.try_add_node("end", "End Node", NodeType::End, None)
        .unwrap();

    g.try_add_edge("start", "stage1", None, 1).unwrap();
    g.try_add_edge("stage1", "stage2", None, 1).unwrap();
    g.try_add_edge("stage2", "end", None, 1).unwrap();
    g
}

// ─── 1. DETERMINISTIC STATE RECONSTRUCTION VIA CHECKPOINTS ──────────────────

#[tokio::test]
async fn test_event_sourcing_checkpoint_state_reconstruction_determinism() {
    let (mut engine, _db, mut ctx, _tmp) = setup_replay_env("replay-det-1").await;

    let (t1, c1) = DeterministicTool::new("t1", 10);
    let (t2, c2) = DeterministicTool::new("t2", 20);

    engine.try_register_tool(Box::new(t1)).unwrap();
    engine.try_register_tool(Box::new(t2)).unwrap();

    let graph = build_pipeline_graph();

    // Run workflow to completion
    engine.run(&mut ctx, &graph).await.unwrap();

    assert_eq!(ctx.status, memfuse_agent::AgentStatus::Completed);
    assert_eq!(c1.load(Ordering::SeqCst), 1);
    assert_eq!(c2.load(Ordering::SeqCst), 1);

    // Save final memory state: stage1 added 10, stage2 added 20 -> acc = 30
    let final_last_output = ctx.memory.get("last_output").cloned();
    assert_eq!(
        final_last_output
            .as_ref()
            .unwrap()
            .get("acc")
            .unwrap()
            .as_u64()
            .unwrap(),
        30
    );

    // Replay to stage2 checkpoint
    engine.replay_from(&mut ctx, "stage2").await.unwrap();
    assert_eq!(ctx.current_node, "stage2");
    assert_eq!(ctx.step_count, 2);

    // Checkpoint memory at stage2: last_output from stage1 (acc = 10)
    let stage1_output = ctx.memory.get("last_output").expect("stage1 output");
    assert_eq!(stage1_output.get("acc").unwrap().as_u64().unwrap(), 10);

    // Now re-execute stage2 from restored state
    engine.run(&mut ctx, &graph).await.unwrap();
    assert_eq!(ctx.status, memfuse_agent::AgentStatus::Completed);

    let reexecuted_output = ctx.memory.get("last_output").cloned();
    assert_eq!(
        final_last_output, reexecuted_output,
        "Re-executed output must match original run output!"
    );
    // t1 was NOT re-executed
    assert_eq!(c1.load(Ordering::SeqCst), 1);
    // t2 was executed twice in total
    assert_eq!(c2.load(Ordering::SeqCst), 2);
}

// ─── 2. DETERMINISTIC AUDIT LOG REPLAY ───────────────────────────────────────

#[tokio::test]
async fn test_audit_log_replay_determinism_and_ordering() {
    let (mut engine, _db, mut ctx, _tmp) = setup_replay_env("replay-audit-det").await;

    let (t1, _) = DeterministicTool::new("t1", 10);
    let (t2, _) = DeterministicTool::new("t2", 20);

    engine.try_register_tool(Box::new(t1)).unwrap();
    engine.try_register_tool(Box::new(t2)).unwrap();

    let graph = build_pipeline_graph();

    engine.run(&mut ctx, &graph).await.unwrap();

    let audit = AuditLog::new(ctx.state_collection.clone());

    let replay_1 = audit.replay_task(&ctx.task_id).await.unwrap();
    let replay_2 = audit.replay_task(&ctx.task_id).await.unwrap();

    assert_eq!(replay_1.len(), 3);
    assert_eq!(replay_2.len(), 3);

    for (a, b) in replay_1.iter().zip(replay_2.iter()) {
        assert_eq!(a.step_count, b.step_count);
        assert_eq!(a.node_id, b.node_id);
        assert_eq!(a.tokens_consumed, b.tokens_consumed);
        assert_eq!(a.payload, b.payload);
        assert_eq!(a.error, b.error);
    }
}
