use memfuse_agent::{
    AgentContext, AgentTool, DeadLetterReason, NodeType, OrchestratorEngine, StateGraph,
    StepResult,
};
use memfuse_core::{MemFuseError, Result, TokenBudget};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

#[tokio::test]
async fn test_tool_timeout_creates_dead_letter() -> Result<()> {
    struct HangingTool;
    #[async_trait::async_trait]
    impl AgentTool for HangingTool {
        fn name(&self) -> &str {
            "hanging"
        }
        fn timeout_ms(&self) -> u64 {
            50 // 50ms Timeout
        }
        async fn execute(&self, _: &AgentContext, _: serde_json::Value) -> Result<StepResult> {
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            unreachable!()
        }
    }

    let temp_dir = tempfile::TempDir::new()?;
    let config = memfuse_db::MemFuseConfig::default();
    let db = Arc::new(memfuse_db::MemFuse::open_with_config(temp_dir.path(), config).await?);
    let state_coll = db.collection("dlq_test_col").await?;

    let mut engine = OrchestratorEngine::from_db(&db);
    engine.try_register_tool(Box::new(HangingTool))?;

    let mut graph = StateGraph::new();
    graph.try_add_node("start", "Start Node", NodeType::Start, None)?;
    graph.try_add_node("task_a", "Hanging Task", NodeType::Task, Some("hanging"))?;
    graph.try_add_node("end", "End Node", NodeType::End, None)?;
    graph.try_add_edge("start", "task_a", None, 1)?;
    graph.try_add_edge("task_a", "end", None, 1)?;

    let mut ctx = AgentContext::try_new(
        "timeout_session",
        "start",
        db,
        state_coll,
        TokenBudget::new(1000, 0),
    )?;

    let res = engine.run(&mut ctx, &graph).await;
    assert!(res.is_err());
    match res {
        Err(MemFuseError::Timeout {
            operation,
            timeout_ms,
        }) => {
            assert_eq!(operation, "tool:hanging");
            assert_eq!(timeout_ms, 50);
        }
        _ => panic!("Expected MemFuseError::Timeout, got {:?}", res),
    }

    let dlq = engine.dead_letter_queue.as_ref().unwrap();
    let letters = dlq.list().await?;
    assert!(!letters.is_empty(), "DLQ should contain at least one entry");

    let letter = &letters[0];
    assert_eq!(letter.session_id, "timeout_session");
    assert_eq!(letter.node_id, "task_a");
    match &letter.failure_reason {
        DeadLetterReason::Timeout { timeout_ms } => {
            assert_eq!(*timeout_ms, 50);
        }
        _ => panic!("Expected DeadLetterReason::Timeout, got {:?}", letter.failure_reason),
    }

    let drained = dlq.drain().await?;
    assert_eq!(drained.len(), letters.len());

    let list_after_drain = dlq.list().await?;
    assert!(list_after_drain.is_empty(), "DLQ should be empty after drain");

    Ok(())
}

#[tokio::test]
async fn test_tool_retry_succeeds_on_second_attempt() -> Result<()> {
    struct FlakeyTool {
        attempt: AtomicU32,
    }
    #[async_trait::async_trait]
    impl AgentTool for FlakeyTool {
        fn name(&self) -> &str {
            "flakey"
        }
        fn is_retriable(&self) -> bool {
            true
        }
        async fn execute(&self, _: &AgentContext, _: serde_json::Value) -> Result<StepResult> {
            let n = self.attempt.fetch_add(1, Ordering::SeqCst);
            if n == 0 {
                Err(MemFuseError::Internal("transient failure".into()))
            } else {
                Ok(StepResult {
                    node_id: "task_a".to_string(),
                    output: serde_json::json!({"status": "ok"}),
                    tokens_consumed: 10,
                    next_edge: None,
                })
            }
        }
    }

    let temp_dir = tempfile::TempDir::new()?;
    let config = memfuse_db::MemFuseConfig::default();
    let db = Arc::new(memfuse_db::MemFuse::open_with_config(temp_dir.path(), config).await?);
    let state_coll = db.collection("dlq_retry_col").await?;

    let mut engine = OrchestratorEngine::from_db(&db);
    let flakey_tool = FlakeyTool {
        attempt: AtomicU32::new(0),
    };
    engine.try_register_tool(Box::new(flakey_tool))?;

    let mut graph = StateGraph::new();
    graph.try_add_node("start", "Start Node", NodeType::Start, None)?;
    graph.try_add_node("task_a", "Flakey Task", NodeType::Task, Some("flakey"))?;
    graph.try_add_node("end", "End Node", NodeType::End, None)?;
    graph.try_add_edge("start", "task_a", None, 1)?;
    graph.try_add_edge("task_a", "end", None, 1)?;

    let mut ctx = AgentContext::try_new(
        "retry_session",
        "start",
        db,
        state_coll,
        TokenBudget::new(1000, 0),
    )?;

    let res = engine.run(&mut ctx, &graph).await;
    assert!(res.is_ok(), "Run should succeed on second attempt");

    let dlq = engine.dead_letter_queue.as_ref().unwrap();
    let letters = dlq.list().await?;
    assert!(letters.is_empty(), "DLQ should be empty when workflow succeeds after retry");

    Ok(())
}
