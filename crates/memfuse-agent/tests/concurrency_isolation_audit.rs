//! Concurrency & Isolation Verification Test Suite for `memfuse-agent`.
//!
//! Verifies parallel execution isolation across independent workflow instances.

use memfuse_agent::audit::AuditLog;
use memfuse_agent::step::StepResult;
use memfuse_agent::{AgentContext, AgentTool, NodeType, OrchestratorEngine, StateGraph};
use memfuse_core::TokenBudget;
use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;

struct ConcurrentTool {
    task_label: String,
}

impl ConcurrentTool {
    fn new(label: &str) -> Self {
        Self {
            task_label: label.to_string(),
        }
    }
}

#[async_trait::async_trait]
impl AgentTool for ConcurrentTool {
    fn name(&self) -> &str {
        "concurrent_tool"
    }

    async fn execute(
        &self,
        _ctx: &AgentContext,
        _input: serde_json::Value,
    ) -> memfuse_core::Result<StepResult> {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        Ok(StepResult {
            node_id: "concurrent_tool".to_string(),
            output: json!({"label": self.task_label}),
            tokens_consumed: 5,
            next_edge: None,
        })
    }
}

fn build_concurrent_graph() -> StateGraph {
    let mut g = StateGraph::new();
    g.try_add_node("start", "Start Node", NodeType::Start, None)
        .unwrap();
    g.try_add_node("step1", "Step 1", NodeType::Task, Some("concurrent_tool"))
        .unwrap();
    g.try_add_node("end", "End Node", NodeType::End, None)
        .unwrap();

    g.try_add_edge("start", "step1", None, 1).unwrap();
    g.try_add_edge("step1", "end", None, 1).unwrap();
    g
}

// ─── 1. PARALLEL EXECUTION ISOLATION TEST ────────────────────────────────────

#[tokio::test]
async fn test_parallel_independent_workflows_isolation() {
    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: 3,
        max_elements: 5_000,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };
    let db = Arc::new(
        MemFuse::open_with_config(tmp.path(), config)
            .await
            .expect("open db"),
    );

    let state_col = db.collection("agent-state").await.expect("collection");
    let num_tasks = 10;
    let mut handles = Vec::new();

    for i in 0..num_tasks {
        let task_id = format!("parallel-task-{i}");
        let db_clone = db.clone();
        let state_col_clone = state_col.clone();

        let handle = tokio::spawn(async move {
            let engine = OrchestratorEngine::from_db(&db_clone);
            let mut tool_engine = engine;
            tool_engine
                .try_register_tool(Box::new(ConcurrentTool::new(&task_id)))
                .unwrap();

            let mut ctx = AgentContext::try_new(
                &task_id,
                "start",
                db_clone,
                state_col_clone,
                TokenBudget::new(1000, 0),
            )
            .unwrap();

            let graph = build_concurrent_graph();
            tool_engine.run(&mut ctx, &graph).await.unwrap();

            // Verify task execution completed
            assert_eq!(ctx.status, memfuse_agent::AgentStatus::Completed);
            assert_eq!(ctx.task_id, task_id);
            ctx
        });

        handles.push(handle);
    }

    let mut contexts = Vec::new();
    for h in handles {
        let ctx = h.await.expect("task thread join");
        contexts.push(ctx);
    }

    assert_eq!(contexts.len(), num_tasks);

    // Verify Audit Log Isolation per Task
    let audit = AuditLog::new(state_col);
    for i in 0..num_tasks {
        let task_id = format!("parallel-task-{i}");
        let entries = audit.replay_task(&task_id).await.unwrap();
        assert_eq!(
            entries.len(),
            2,
            "Task {task_id} must have exactly 2 audit entries (start + step1)"
        );
        for entry in &entries {
            assert_eq!(entry.task_id, task_id);
        }
    }
}
