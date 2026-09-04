//! Integration tests for continuous event loop and event sources.

use async_trait::async_trait;
use memfuse_agent::event_source::{
    BackgroundEvent, EphemeralEventSource, EventSource, PollingDocumentEventSource, VecEventSource,
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
    engine.try_register_tool(Box::new(TelemetryTool)).unwrap();

    (engine, db, tmp)
}

#[tokio::test]
async fn test_event_loop_exhausted_source_exit_and_checkpointing() {
    let (engine, db, _tmp) = setup_test_environment().await;

    let mut graph = StateGraph::new();
    graph
        .try_add_node("start", "Start node", NodeType::Start, None)
        .unwrap();
    graph
        .try_add_node(
            "task",
            "Process event",
            NodeType::Task,
            Some("telemetry_tool"),
        )
        .unwrap();
    graph
        .try_add_node("end", "Finish step", NodeType::End, None)
        .unwrap();

    graph.try_add_edge("start", "task", None, 1).unwrap();
    graph.try_add_edge("task", "end", None, 1).unwrap();

    let state_col = db.collection("agent-state").await.expect("state col"); // unwrap allowed
    let budget = TokenBudget::new(1000, 0);
    let mut ctx =
        AgentContext::try_new("task-evt-1", "start", db.clone(), state_col, budget).unwrap();

    let events = vec![
        BackgroundEvent::try_new(json!({"type": "log", "msg": "event 1"}), "log_stream", 1)
            .unwrap(),
        BackgroundEvent::try_new(json!({"type": "log", "msg": "event 2"}), "log_stream", 2)
            .unwrap(),
        BackgroundEvent::try_new(json!({"type": "log", "msg": "event 3"}), "log_stream", 3)
            .unwrap(),
    ];
    let mut source = VecEventSource::try_new(events).unwrap();
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
    graph
        .try_add_node("start", "Start node", NodeType::Start, None)
        .unwrap();
    graph
        .try_add_node("end", "End node", NodeType::End, None)
        .unwrap();
    graph.try_add_edge("start", "end", None, 1).unwrap();

    let state_col = db.collection("agent-state").await.expect("state col"); // unwrap allowed
    let budget = TokenBudget::new(1000, 0);
    let mut ctx =
        AgentContext::try_new("task-cancel-1", "start", db.clone(), state_col, budget).unwrap();

    let shutdown = CancellationToken::new();
    shutdown.cancel();

    let mut source = VecEventSource::try_new(vec![BackgroundEvent::try_new(
        json!({"data": "ignored"}),
        "test",
        1,
    )
    .unwrap()])
    .unwrap();

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
async fn test_event_loop_no_busy_wait() {
    let (engine, db, _tmp) = setup_test_environment().await;

    let mut graph = StateGraph::new();
    graph
        .try_add_node("start", "Start node", NodeType::Start, None)
        .unwrap();
    graph
        .try_add_node(
            "task",
            "Process event",
            NodeType::Task,
            Some("telemetry_tool"),
        )
        .unwrap();
    graph
        .try_add_node("end", "Finish step", NodeType::End, None)
        .unwrap();

    graph.try_add_edge("start", "task", None, 1).unwrap();
    graph.try_add_edge("task", "end", None, 1).unwrap();

    let state_col = db.collection("agent-state").await.expect("state col");
    let budget = TokenBudget::new(1000, 0);
    let mut ctx =
        AgentContext::try_new("task-no-busy-wait", "start", db.clone(), state_col, budget).unwrap();

    let (mut source, producer) = EphemeralEventSource::new();
    let shutdown = CancellationToken::new();

    let poll_count_handle = source.poll_count_handle();

    let producer_task = tokio::spawn(async move {
        for i in 1..=10 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let event = BackgroundEvent::try_new(
                json!({"type": "metric", "seq": i}),
                "sensor_stream",
                i as u64,
            )
            .unwrap();
            producer.push(event);
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        producer.close();
    });

    let start_time = std::time::Instant::now();

    let exit_reason = engine
        .run_event_loop(&mut ctx, &graph, &mut source, shutdown)
        .await
        .expect("run event loop");

    let elapsed = start_time.elapsed();
    producer_task.await.expect("producer task finish");

    assert_eq!(exit_reason, EventLoopExitReason::SourceExhausted);
    assert_eq!(ctx.events.len(), 10);

    let total_polls = poll_count_handle.load(std::sync::atomic::Ordering::SeqCst);
    let idle_time_secs = elapsed.as_secs_f64();

    let wakeups_per_sec = total_polls as f64 / idle_time_secs;
    assert!(
        total_polls <= 25,
        "Expected total polls <= 25 for 10 events over ~1s, got {total_polls} (rate: {wakeups_per_sec:.2} polls/sec)"
    );
}

#[tokio::test]
async fn test_custom_event_source_extensibility() {
    let (engine, db, _tmp) = setup_test_environment().await;

    let mut graph = StateGraph::new();
    graph
        .try_add_node("start", "Start node", NodeType::Start, None)
        .unwrap();
    graph
        .try_add_node("end", "End node", NodeType::End, None)
        .unwrap();
    graph.try_add_edge("start", "end", None, 1).unwrap();

    let state_col = db.collection("agent-state").await.expect("state col"); // unwrap allowed
    let budget = TokenBudget::new(1000, 0);
    let mut ctx =
        AgentContext::try_new("task-custom-1", "start", db.clone(), state_col, budget).unwrap();

    let mut custom_source = CustomStreamSource {
        stream: vec![
            BackgroundEvent::try_new(json!({"screen": "frame_001"}), "camera", 10).unwrap(),
        ],
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
