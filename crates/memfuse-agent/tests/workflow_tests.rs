use memfuse_core::BoxFuture;
use memfuse_agent::{NodeType, StateGraph};

#[tokio::test]
async fn test_stategraph_construction() {
    let mut graph = StateGraph::new();
    graph
        .try_add_node(
            "research",
            "Researching...",
            NodeType::Task,
            Some("search_tool"),
        )
        .unwrap();
    graph
        .try_add_node(
            "code",
            "Generating code...",
            NodeType::Task,
            Some("code_gen_tool"),
        )
        .unwrap();

    graph
        .try_add_edge("research", "code", Some("research_complete"), 1)
        .unwrap();

    assert_eq!(graph.nodes.len(), 2);
    assert_eq!(graph.edges.len(), 1);
}

#[test]
fn test_stategraph_boundary_validation() {
    let mut graph = StateGraph::new();

    // Empty Node ID validation
    let node_res = graph.try_add_node("", "Invalid empty node", NodeType::Task, None);
    assert!(node_res.is_err());
    if let Err(memfuse_core::MemFuseError::InvalidInput(msg)) = node_res {
        assert!(msg.contains("node_id cannot be empty"));
    } else {
        panic!("Expected InvalidInput error");
    }

    // Empty Node description validation
    let desc_res = graph.try_add_node("node_x", "", NodeType::Task, None);
    assert!(desc_res.is_err());
    if let Err(memfuse_core::MemFuseError::InvalidInput(msg)) = desc_res {
        assert!(msg.contains("StateGraph node description must not be empty"));
    } else {
        panic!("Expected InvalidInput error");
    }

    // Empty Edge endpoint validation
    let edge_res1 = graph.try_add_edge("", "node_b", None, 1);
    assert!(edge_res1.is_err());

    let edge_res2 = graph.try_add_edge("node_a", "   ", None, 1);
    assert!(edge_res2.is_err());

    // Valid node & edge addition
    assert!(graph
        .try_add_node("node_a", "Valid node A", NodeType::Start, None)
        .is_ok());
    assert!(graph
        .try_add_node("node_b", "Valid node B", NodeType::End, None)
        .is_ok());
    assert!(graph.try_add_edge("node_a", "node_b", None, 1).is_ok());

    assert_eq!(graph.nodes.len(), 2);
    assert_eq!(graph.edges.len(), 1);
}

#[test]
fn test_background_event_boundary_validation() {
    use memfuse_agent::event_source::BackgroundEvent;

    let empty_source_res = BackgroundEvent::try_new(serde_json::json!({"test": 1}), "", 10);
    assert!(empty_source_res.is_err());

    let whitespace_source_res = BackgroundEvent::try_new(serde_json::json!({"test": 1}), "   ", 10);
    assert!(whitespace_source_res.is_err());

    let valid_evt = BackgroundEvent::try_new(serde_json::json!({"test": 1}), "source_a", 10);
    assert!(valid_evt.is_ok());
    assert_eq!(valid_evt.unwrap().source, "source_a");
}

#[tokio::test]
async fn test_agent_context_boundary_validation() {
    use memfuse_agent::context::AgentContext;
    use memfuse_core::TokenBudget;
    use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
    use std::sync::Arc;
    use tempfile::TempDir;

    let tmp = TempDir::new().expect("failed to create temp dir");
    let config = MemFuseConfig {
        dimension: 3,
        max_elements: 1000,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };
    let db = Arc::new(
        MemFuse::open_with_config(tmp.path(), config)
            .await
            .expect("failed to open db"),
    );
    let state_col = db.collection("agent_state").await.expect("col failed");

    // Empty task_id validation
    let err_task = AgentContext::try_new(
        "",
        "start_node",
        db.clone(),
        state_col.clone(),
        TokenBudget::new(100, 0),
    );
    assert!(err_task.is_err());

    // Empty start_node validation
    let err_node = AgentContext::try_new(
        "task_1",
        "",
        db.clone(),
        state_col.clone(),
        TokenBudget::new(100, 0),
    );
    assert!(err_node.is_err());

    // Valid AgentContext creation
    let valid_ctx = AgentContext::try_new(
        "task_1",
        "start_node",
        db,
        state_col,
        TokenBudget::new(100, 0),
    );
    assert!(valid_ctx.is_ok());
}

/// Mock tool tracking execution count.
struct CountingTool {
    name: String,
    tokens: usize,
    call_count: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl memfuse_agent::AgentTool for CountingTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn execute<'a>(
        &'a self,
        _ctx: &'a memfuse_agent::AgentContext,
        _input: serde_json::Value,
    ) -> BoxFuture<'a, memfuse_core::Result<memfuse_agent::StepResult>> {
        Box::pin(async move {
            self.call_count
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(memfuse_agent::StepResult {
                node_id: self.name.clone(),
                output: serde_json::json!({"status": "ok"}),
                tokens_consumed: self.tokens,
                next_edge: None,
            })
        })
    }
}

#[tokio::test]
async fn test_pre_execution_budget_check_prevents_tool_execution() {
    use memfuse_agent::context::AgentContext;
    use memfuse_agent::{NodeType, OrchestratorEngine, StateGraph};
    use memfuse_core::TokenBudget;
    use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
    use std::sync::Arc;
    use tempfile::TempDir;

    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: 3,
        max_elements: 1000,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };
    let db = Arc::new(
        MemFuse::open_with_config(tmp.path(), config)
            .await
            .expect("open db"),
    );
    let state_col = db.collection("agent_state").await.expect("col");

    // Total budget = 10 tokens. Each step tool consumes 10 tokens.
    // Start node consumes 0 tokens.
    // Task 1 consumes 10 tokens (budget becomes 0).
    // Task 2 attempts to run, but budget is exhausted (0 available).
    // With pre-check, Task 2 tool must NOT be executed!
    let mut ctx = AgentContext::try_new(
        "task-budget-precheck",
        "start",
        db.clone(),
        state_col,
        TokenBudget::new(10, 0),
    )
    .expect("ctx");

    let counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    let mut graph = StateGraph::new();
    graph
        .try_add_node("start", "Start Node", NodeType::Start, None)
        .unwrap();
    graph
        .try_add_node("task_1", "Task Node 1", NodeType::Task, Some("count_tool"))
        .unwrap();
    graph
        .try_add_node("task_2", "Task Node 2", NodeType::Task, Some("count_tool"))
        .unwrap();
    graph
        .try_add_node("end", "End Node", NodeType::End, None)
        .unwrap();

    graph.try_add_edge("start", "task_1", None, 1).unwrap();
    graph.try_add_edge("task_1", "task_2", None, 1).unwrap();
    graph.try_add_edge("task_2", "end", None, 1).unwrap();

    let mut engine = OrchestratorEngine::new(db.inner_storage());
    engine
        .try_register_tool(Box::new(CountingTool {
            name: "count_tool".to_string(),
            tokens: 10,
            call_count: counter.clone(),
        }))
        .unwrap();

    let res = engine.run(&mut ctx, &graph).await;
    assert!(res.is_err(), "Run should fail due to budget exhaustion");
    let err_msg = res.unwrap_err().to_string();
    assert!(
        err_msg.contains("Token budget exhausted before step execution"),
        "Expected pre-execution error, got: {}",
        err_msg
    );

    // Verify counter: task_1 ran once. task_2 was stopped BEFORE execution, so call_count == 1 (not 2).
    assert_eq!(
        counter.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "Tool must execute exactly once (task_1), task_2 tool execution must be skipped"
    );
}

#[tokio::test]
async fn test_replay_from_restores_budget_state() {
    use memfuse_agent::context::AgentContext;
    use memfuse_agent::{NodeType, OrchestratorEngine, StateGraph};
    use memfuse_core::TokenBudget;
    use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
    use std::sync::Arc;
    use tempfile::TempDir;

    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: 3,
        max_elements: 1000,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };
    let db = Arc::new(
        MemFuse::open_with_config(tmp.path(), config)
            .await
            .expect("open db"),
    );
    let state_col = db.collection("agent_state").await.expect("col");

    // Total budget = 100 tokens.
    let mut ctx = AgentContext::try_new(
        "task-budget-replay",
        "start",
        db.clone(),
        state_col,
        TokenBudget::new(100, 0),
    )
    .expect("ctx");

    let counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    let mut graph = StateGraph::new();
    graph
        .try_add_node("start", "Start Node", NodeType::Start, None)
        .unwrap();
    graph
        .try_add_node("step_a", "Step A", NodeType::Task, Some("count_tool"))
        .unwrap();
    graph
        .try_add_node("end", "End Node", NodeType::End, None)
        .unwrap();

    graph.try_add_edge("start", "step_a", None, 1).unwrap();
    graph.try_add_edge("step_a", "end", None, 1).unwrap();

    let mut engine = OrchestratorEngine::new(db.inner_storage());
    engine
        .try_register_tool(Box::new(CountingTool {
            name: "count_tool".to_string(),
            tokens: 30,
            call_count: counter,
        }))
        .unwrap();

    engine.run(&mut ctx, &graph).await.expect("run");
    assert_eq!(ctx.budget.consumed(), 30);
    assert_eq!(ctx.budget.available(), 70);

    // Now replay_from step_a
    engine
        .replay_from(&mut ctx, "step_a")
        .await
        .expect("replay");

    // At step_a checkpoint (before step_a execution), consumed was 0, available was 100
    assert_eq!(ctx.budget.consumed(), 0);
    assert_eq!(ctx.budget.available(), 100);
}

#[tokio::test]
async fn test_replay_from_identifier_resolution() {
    use memfuse_agent::context::AgentContext;
    use memfuse_agent::{NodeType, OrchestratorEngine, StateGraph};
    use memfuse_core::TokenBudget;
    use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
    use std::sync::Arc;
    use tempfile::TempDir;

    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: 3,
        max_elements: 1000,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };
    let db = Arc::new(
        MemFuse::open_with_config(tmp.path(), config)
            .await
            .expect("open db"),
    );
    let state_col = db.collection("agent_state").await.expect("col");

    let mut ctx = AgentContext::try_new(
        "task-identifier-test",
        "1", // Node named "1"
        db.clone(),
        state_col.clone(),
        TokenBudget::new(100, 0),
    )
    .expect("ctx");

    let mut graph = StateGraph::new();
    // Node is named "1"
    graph
        .try_add_node("1", "Node named 1", NodeType::Start, None)
        .unwrap();
    graph
        .try_add_node("step_b", "Step B", NodeType::Task, Some("count_tool"))
        .unwrap();
    graph
        .try_add_node("end", "End Node", NodeType::End, None)
        .unwrap();

    graph.try_add_edge("1", "step_b", None, 1).unwrap();
    graph.try_add_edge("step_b", "end", None, 1).unwrap();

    let counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    let mut engine = OrchestratorEngine::new(db.inner_storage());
    engine
        .try_register_tool(Box::new(CountingTool {
            name: "count_tool".to_string(),
            tokens: 5,
            call_count: counter,
        }))
        .unwrap();

    engine.run(&mut ctx, &graph).await.expect("run");

    // Checkpoints created:
    // step 0: node 1
    // step 1: node step_b
    // step 2: node end

    // Test resolution formats against checkpoints:
    // 1. Explicit step addressing: replay_from "step:0" -> current_node == "1", step_count == 0
    let mut ctx1 = ctx;
    engine
        .replay_from(&mut ctx1, "step:0")
        .await
        .expect("replay step:0");
    assert_eq!(ctx1.current_node, "1");
    assert_eq!(ctx1.step_count, 0);

    // Re-run workflow for fresh state for subsequent tests
    let mut ctx2 = AgentContext::try_new(
        "task-identifier-test-2",
        "1",
        db.clone(),
        state_col.clone(),
        TokenBudget::new(100, 0),
    )
    .expect("ctx2");
    engine.run(&mut ctx2, &graph).await.expect("run 2");

    // 2. Explicit node addressing: replay_from "node:1" -> current_node == "1", step_count == 0
    engine
        .replay_from(&mut ctx2, "node:1")
        .await
        .expect("replay node:1");
    assert_eq!(ctx2.current_node, "1");
    assert_eq!(ctx2.step_count, 0);

    // Re-run workflow for fresh state for test 3
    let mut ctx3 = AgentContext::try_new(
        "task-identifier-test-3",
        "1",
        db.clone(),
        state_col.clone(),
        TokenBudget::new(100, 0),
    )
    .expect("ctx3");
    engine.run(&mut ctx3, &graph).await.expect("run 3");

    // 3. Fallback un-prefixed numeric addressing: replay_from "1" -> matches step 1 (node step_b)
    engine
        .replay_from(&mut ctx3, "1")
        .await
        .expect("replay fallback step 1");
    assert_eq!(ctx3.current_node, "step_b");
    assert_eq!(ctx3.step_count, 1);

    // Re-run workflow for fresh state for test 4 & 5
    let mut ctx4 = AgentContext::try_new(
        "task-identifier-test-4",
        "1",
        db.clone(),
        state_col,
        TokenBudget::new(100, 0),
    )
    .expect("ctx4");
    engine.run(&mut ctx4, &graph).await.expect("run 4");

    // 4. Fallback non-numeric addressing: replay_from "step_b" -> matches node step_b
    engine
        .replay_from(&mut ctx4, "step_b")
        .await
        .expect("replay fallback step_b");
    assert_eq!(ctx4.current_node, "step_b");
    assert_eq!(ctx4.step_count, 1);

    // 5. Non-existent numeric step addressing error contains hint
    let err = engine.replay_from(&mut ctx4, "99").await.unwrap_err();
    let err_msg = err.to_string();
    assert!(
        err_msg.contains("Konnte keinen Checkpoint für Schritt 99 finden. Falls ein Node mit dem Namen '99' gemeint war, nutze das Format 'node:99' zur expliziten Adressierung."),
        "Error message should contain helpful node addressing hint, got: {}",
        err_msg
    );
}

#[tokio::test]
async fn test_audit_log_field_reused_across_steps() {
    use memfuse_agent::audit::AuditLog;
    use memfuse_agent::context::AgentContext;
    use memfuse_agent::{NodeType, OrchestratorEngine, StateGraph};
    use memfuse_core::TokenBudget;
    use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
    use std::sync::Arc;
    use tempfile::TempDir;

    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: 3,
        max_elements: 1000,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };
    let db = Arc::new(
        MemFuse::open_with_config(tmp.path(), config)
            .await
            .expect("open db"),
    );
    let state_col = db.collection("agent_state_reuse").await.expect("col");

    // 5-step workflow: start -> step_1 -> step_2 -> step_3 -> step_4 -> end
    let mut graph = StateGraph::new();
    graph
        .try_add_node("start", "Start Node", NodeType::Start, None)
        .unwrap();
    graph
        .try_add_node("step_1", "Step 1", NodeType::Task, Some("step_tool"))
        .unwrap();
    graph
        .try_add_node("step_2", "Step 2", NodeType::Task, Some("step_tool"))
        .unwrap();
    graph
        .try_add_node("step_3", "Step 3", NodeType::Task, Some("step_tool"))
        .unwrap();
    graph
        .try_add_node("step_4", "Step 4", NodeType::Task, Some("step_tool"))
        .unwrap();
    graph
        .try_add_node("end", "End Node", NodeType::End, None)
        .unwrap();

    graph.try_add_edge("start", "step_1", None, 1).unwrap();
    graph.try_add_edge("step_1", "step_2", None, 1).unwrap();
    graph.try_add_edge("step_2", "step_3", None, 1).unwrap();
    graph.try_add_edge("step_3", "step_4", None, 1).unwrap();
    graph.try_add_edge("step_4", "end", None, 1).unwrap();

    let counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    let mut engine = OrchestratorEngine::new(db.inner_storage());
    engine
        .try_register_tool(Box::new(CountingTool {
            name: "step_tool".to_string(),
            tokens: 1,
            call_count: counter.clone(),
        }))
        .unwrap();

    let mut ctx = AgentContext::try_new(
        "task-reuse-test",
        "start",
        db.clone(),
        state_col.clone(),
        TokenBudget::new(100, 0),
    )
    .expect("ctx");

    let count_before = Arc::strong_count(&state_col);

    // Run 5-step workflow
    engine.run(&mut ctx, &graph).await.expect("workflow run");

    let count_after = Arc::strong_count(&state_col);

    // Assert that collection Arc strong count did not grow per step across workflow execution
    assert_eq!(
        count_before, count_after,
        "Arc strong count on collection must remain identical before and after workflow execution (no reference leaks or per-step Arc clones)"
    );

    // Verify all 5 steps were executed and audit trail captured all steps
    assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 4); // 4 task nodes called tool
    let audit_log = AuditLog::new(state_col);
    let entries = audit_log
        .replay_task("task-reuse-test")
        .await
        .expect("replay");
    assert_eq!(
        entries.len(),
        5,
        "Audit log should contain entries for all 5 executed steps"
    );
}
