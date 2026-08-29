//! Integration tests for continuous event loop and event sources.

use async_trait::async_trait;
use memfuse_agent::event_source::{
    BackgroundEvent, EventSource, PollingDocumentEventSource, VecEventSource,
};
use memfuse_agent::step::StepResult;
use memfuse_agent::{AgentContext, EventLoopExitReason, NodeType, OrchestratorEngine, StateGraph};
use memfuse_core::TokenBudget;
use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

/// Tool that captures the current event and returns a result.
struct TelemetryTool;

#[async_trait]
impl memfuse_agent::AgentTool for TelemetryTool {
    fn name(&self) -> &str {
        "telemetry_tool"
    }

    async fn execute(
        &self,
        ctx: &AgentContext,
        _input: serde_json::Value,
    ) -> memfuse_core::Result<StepResult> {
        let latest = ctx
            .memory
            .get("latest_event")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        Ok(StepResult {
            node_id: "telemetry_step".to_string(),
            output: json!({"processed_event": latest}),
            tokens_consumed: 2,
            next_edge: None,
        })
    }
}

async fn setup_test_environment() -> (OrchestratorEngine, Arc<MemFuse>, TempDir) {
    let tmp = TempDir::new().expect("temp dir"); // unwrap allowed
    let config = MemFuseConfig {
        dimension: 4,
        max_elements: 1000,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };
    let db = Arc::new(
        MemFuse::open_with_config(tmp.path(), config)
            .await
            .expect("open db"), // unwrap allowed
    );

    let storage = db.inner_storage();
    let mut engine = OrchestratorEngine::new(storage);
    engine.register_tool(Box::new(TelemetryTool));

    (engine, db, tmp)
}

#[tokio::test]
async fn test_event_loop_exhausted_source_exit_and_checkpointing() {
    let (engine, db, _tmp) = setup_test_environment().await;

    let mut graph = StateGraph::new();
    graph.add_node("start", "Start node", NodeType::Start, None);
    graph.add_node(
        "task",
        "Process event",
        NodeType::Task,
        Some("telemetry_tool"),
    );
    graph.add_node("end", "Finish step", NodeType::End, None);

    graph.add_edge("start", "task", None, 1);
    graph.add_edge("task", "end", None, 1);

    let state_col = db.collection("agent-state").await.expect("state col"); // unwrap allowed
    let budget = TokenBudget::new(1000, 0);
    let mut ctx = AgentContext::new("task-evt-1", "start", db.clone(), state_col, budget);

    let events = vec![
        BackgroundEvent::new(json!({"type": "log", "msg": "event 1"}), "log_stream", 1),
        BackgroundEvent::new(json!({"type": "log", "msg": "event 2"}), "log_stream", 2),
        BackgroundEvent::new(json!({"type": "log", "msg": "event 3"}), "log_stream", 3),
    ];
    let mut source = VecEventSource::new(events);
    let shutdown = CancellationToken::new();

    let exit_reason = engine
        .run_event_loop(&mut ctx, &graph, &mut source, shutdown)
        .await
        .expect("run event loop"); // unwrap allowed

    assert_eq!(exit_reason, EventLoopExitReason::SourceExhausted);
    assert_eq!(ctx.events.len(), 3);
    assert_eq!(ctx.events[0].payload["msg"], "event 1");
    assert_eq!(ctx.events[1].payload["msg"], "event 2");
    assert_eq!(ctx.events[2].payload["msg"], "event 3");

    // Verify checkpoint store has recorded checkpoints for each processed event
    let checkpoints = engine
        .checkpoint_store
        .list_checkpoints()
        .await
        .expect("list checkpoints"); // unwrap allowed
    assert!(checkpoints.len() >= 3);
}

#[tokio::test]
async fn test_event_loop_cancellation_token_exit() {
    let (engine, db, _tmp) = setup_test_environment().await;

    let mut graph = StateGraph::new();
    graph.add_node("start", "Start node", NodeType::Start, None);
    graph.add_node("end", "End node", NodeType::End, None);
    graph.add_edge("start", "end", None, 1);

    let state_col = db.collection("agent-state").await.expect("state col"); // unwrap allowed
    let budget = TokenBudget::new(1000, 0);
    let mut ctx = AgentContext::new("task-cancel-1", "start", db.clone(), state_col, budget);

    let shutdown = CancellationToken::new();
    shutdown.cancel();

    let mut source = VecEventSource::new(vec![BackgroundEvent::new(
        json!({"data": "ignored"}),
        "test",
        1,
    )]);

    let exit_reason = engine
        .run_event_loop(&mut ctx, &graph, &mut source, shutdown)
        .await
        .expect("run event loop"); // unwrap allowed

    assert_eq!(exit_reason, EventLoopExitReason::Shutdown);
}

#[tokio::test]
async fn test_polling_document_event_source_delta_detection() {
    let tmp = TempDir::new().expect("temp dir"); // unwrap allowed
    let config = MemFuseConfig {
        dimension: 4,
        max_elements: 1000,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };
    let db = Arc::new(
        MemFuse::open_with_config(tmp.path(), config)
            .await
            .expect("open db"), // unwrap allowed
    );

    let doc_col = db.collection("telemetry-docs").await.expect("doc col"); // unwrap allowed
    let mut source = PollingDocumentEventSource::new(doc_col.clone(), Duration::from_millis(10));

    // Initially no events
    let initial_evt = source.next_event().await.expect("next event"); // unwrap allowed
    assert!(initial_evt.is_none());

    // Insert first document
    doc_col
        .insert(
            "doc-1",
            &[1.0, 0.0, 0.0, 0.0],
            Some(json!({"text": "first"})),
        )
        .await
        .expect("insert doc 1"); // unwrap allowed

    let evt1 = source
        .next_event()
        .await
        .expect("next event") // unwrap allowed
        .expect("event 1 present"); // unwrap allowed

    assert_eq!(evt1.source, "collection:telemetry-docs");
    assert!(evt1.observed_at_seq > 0);

    // Polling again without changes returns None
    let empty_evt = source.next_event().await.expect("next event"); // unwrap allowed
    assert!(empty_evt.is_none());

    // Insert second document
    doc_col
        .insert(
            "doc-2",
            &[0.0, 1.0, 0.0, 0.0],
            Some(json!({"text": "second"})),
        )
        .await
        .expect("insert doc 2"); // unwrap allowed

    let evt2 = source
        .next_event()
        .await
        .expect("next event") // unwrap allowed
        .expect("event 2 present"); // unwrap allowed

    assert_eq!(evt2.source, "collection:telemetry-docs");
    assert!(evt2.observed_at_seq > evt1.observed_at_seq);
}

/// A second trivial custom EventSource to verify polymorphism and extensibility.
struct CustomStreamSource {
    stream: Vec<BackgroundEvent>,
    idx: usize,
}

#[async_trait]
impl EventSource for CustomStreamSource {
    async fn next_event(&mut self) -> memfuse_core::Result<Option<BackgroundEvent>> {
        if self.idx < self.stream.len() {
            let item = self.stream[self.idx].clone();
            self.idx += 1;
            Ok(Some(item))
        } else {
            Ok(None)
        }
    }

    fn is_exhausted(&self) -> bool {
        self.idx >= self.stream.len()
    }
}

#[tokio::test]
async fn test_custom_event_source_extensibility() {
    let (engine, db, _tmp) = setup_test_environment().await;

    let mut graph = StateGraph::new();
    graph.add_node("start", "Start node", NodeType::Start, None);
    graph.add_node("end", "End node", NodeType::End, None);
    graph.add_edge("start", "end", None, 1);

    let state_col = db.collection("agent-state").await.expect("state col"); // unwrap allowed
    let budget = TokenBudget::new(1000, 0);
    let mut ctx = AgentContext::new("task-custom-1", "start", db.clone(), state_col, budget);

    let mut custom_source = CustomStreamSource {
        stream: vec![BackgroundEvent::new(
            json!({"screen": "frame_001"}),
            "camera",
            10,
        )],
        idx: 0,
    };
    let shutdown = CancellationToken::new();

    let exit_reason = engine
        .run_event_loop(&mut ctx, &graph, &mut custom_source, shutdown)
        .await
        .expect("run event loop"); // unwrap allowed

    assert_eq!(exit_reason, EventLoopExitReason::SourceExhausted);
    assert_eq!(ctx.events.len(), 1);
    assert_eq!(ctx.events[0].source, "camera");
}
