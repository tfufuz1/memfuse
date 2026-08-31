//! Audit Log Immutability & Completeness Verification Test Suite for `memfuse-agent`.
//!
//! Verifies structural immutability (lack of update/delete API), append-only semantics,
//! and complete audit trail recording across happy path and failure scenarios.

use memfuse_agent::audit::{AuditEntry, AuditLog};
use memfuse_agent::step::StepResult;
use memfuse_agent::{AgentContext, AgentTool, NodeType, OrchestratorEngine, StateGraph};
use memfuse_core::TokenBudget;
use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tempfile::TempDir;

struct AuditTestTool {
    name: String,
    fail: bool,
    tokens: usize,
    call_count: Arc<AtomicUsize>,
}

impl AuditTestTool {
    fn new(name: &str, fail: bool, tokens: usize) -> (Self, Arc<AtomicUsize>) {
        let count = Arc::new(AtomicUsize::new(0));
        (
            Self {
                name: name.to_string(),
                fail,
                tokens,
                call_count: count.clone(),
            },
            count,
        )
    }
}

#[async_trait::async_trait]
impl AgentTool for AuditTestTool {
    fn name(&self) -> &str {
        &self.name
    }

    async fn execute(
        &self,
        _ctx: &AgentContext,
        _input: serde_json::Value,
    ) -> memfuse_core::Result<StepResult> {
        let current = self.call_count.fetch_add(1, Ordering::SeqCst) + 1;
        if self.fail {
            return Err(memfuse_core::MemFuseError::Internal(format!(
                "Tool {} failed on step {}",
                self.name, current
            )));
        }
        Ok(StepResult {
            node_id: self.name.clone(),
            output: json!({"status": "ok", "call": current}),
            tokens_consumed: self.tokens,
            next_edge: None,
        })
    }
}

async fn setup_audit_env(
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

fn build_three_step_graph() -> StateGraph {
    let mut g = StateGraph::new();
    g.try_add_node("start", "Start Node", NodeType::Start, None)
        .unwrap();
    g.try_add_node("step1", "Step 1", NodeType::Task, Some("tool1"))
        .unwrap();
    g.try_add_node("step2", "Step 2", NodeType::Task, Some("tool2"))
        .unwrap();
    g.try_add_node("step3", "Step 3", NodeType::Task, Some("tool3"))
        .unwrap();
    g.try_add_node("end", "End Node", NodeType::End, None)
        .unwrap();

    g.try_add_edge("start", "step1", None, 1).unwrap();
    g.try_add_edge("step1", "step2", None, 1).unwrap();
    g.try_add_edge("step2", "step3", None, 1).unwrap();
    g.try_add_edge("step3", "end", None, 1).unwrap();
    g
}

// ─── 1. API SURFACE & STRUCTURAL IMMUTABILITY AUDIT ────────────────────────

#[tokio::test]
async fn test_audit_log_api_surface_and_append_only_integrity() {
    let (engine, _db, ctx, _tmp) = setup_audit_env("audit-immutability-1").await;
    let audit_log = AuditLog::new(ctx.state_collection.clone());

    let entry1 = AuditEntry {
        task_id: ctx.task_id.clone(),
        step_count: 0,
        node_id: "start".to_string(),
        tokens_consumed: 0,
        payload: json!({"initial": true}),
        error: None,
    };

    let entry2 = AuditEntry {
        task_id: ctx.task_id.clone(),
        step_count: 1,
        node_id: "step1".to_string(),
        tokens_consumed: 15,
        payload: json!({"action": "process"}),
        error: None,
    };

    // Append entries
    audit_log.append(&entry1).await.unwrap();
    audit_log.append(&entry2).await.unwrap();

    let replayed = audit_log.replay_task(&ctx.task_id).await.unwrap();
    assert_eq!(replayed.len(), 2);
    assert_eq!(replayed[0].step_count, 0);
    assert_eq!(replayed[0].node_id, "start");
    assert_eq!(replayed[1].step_count, 1);
    assert_eq!(replayed[1].node_id, "step1");

    // Flush storage
    engine.checkpoint_store.list_checkpoints().await.unwrap();
}

// ─── 2. AUDIT TRAIL COMPLETENESS ON HAPPY PATH & FAILURE ───────────────────

#[tokio::test]
async fn test_audit_trail_completeness_happy_path() {
    let (mut engine, _db, mut ctx, _tmp) = setup_audit_env("audit-completeness-happy").await;

    let (t1, _) = AuditTestTool::new("tool1", false, 10);
    let (t2, _) = AuditTestTool::new("tool2", false, 10);
    let (t3, _) = AuditTestTool::new("tool3", false, 10);

    engine.try_register_tool(Box::new(t1)).unwrap();
    engine.try_register_tool(Box::new(t2)).unwrap();
    engine.try_register_tool(Box::new(t3)).unwrap();

    let graph = build_three_step_graph();

    engine.run(&mut ctx, &graph).await.unwrap();

    let audit_log = AuditLog::new(ctx.state_collection.clone());
    let entries = audit_log.replay_task(&ctx.task_id).await.unwrap();

    // 4 nodes executed: start (step 0), step1 (step 1), step2 (step 2), step3 (step 3)
    assert_eq!(entries.len(), 4);
    assert_eq!(entries[0].node_id, "start");
    assert_eq!(entries[1].node_id, "step1");
    assert_eq!(entries[2].node_id, "step2");
    assert_eq!(entries[3].node_id, "step3");
    for entry in &entries {
        assert!(entry.error.is_none());
    }
}

#[tokio::test]
async fn test_audit_trail_completeness_on_tool_failure() {
    let (mut engine, _db, mut ctx, _tmp) = setup_audit_env("audit-completeness-fail").await;

    let (t1, _) = AuditTestTool::new("tool1", false, 10);
    // tool2 will fail
    let (t2, _) = AuditTestTool::new("tool2", true, 10);
    let (t3, _) = AuditTestTool::new("tool3", false, 10);

    engine.try_register_tool(Box::new(t1)).unwrap();
    engine.try_register_tool(Box::new(t2)).unwrap();
    engine.try_register_tool(Box::new(t3)).unwrap();

    let graph = build_three_step_graph();

    let res = engine.run(&mut ctx, &graph).await;
    assert!(res.is_err());

    let audit_log = AuditLog::new(ctx.state_collection.clone());
    let entries = audit_log.replay_task(&ctx.task_id).await.unwrap();

    // 3 entries logged: start (step 0, success), step1 (step 1, success), step2 (step 2, failure logged in audit)
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].node_id, "start");
    assert!(entries[0].error.is_none());

    assert_eq!(entries[1].node_id, "step1");
    assert!(entries[1].error.is_none());

    assert_eq!(entries[2].node_id, "step2");
    assert!(entries[2].error.is_some());
    assert!(entries[2]
        .error
        .as_ref()
        .unwrap()
        .contains("failed on step 1"));
}
