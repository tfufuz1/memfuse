//! E2E integration tests for memfuse-saos-agent.
//!
//! Validates the full stack: MemFuse DB → Collection → OrchestratorEngine → Graph walk.

use memfuse_core::TokenBudget;
use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use memfuse_saos_agent::step::StepResult;
use memfuse_saos_agent::{AgentContext, NodeType, OrchestratorEngine, StateGraph};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;

/// A trivial tool that echoes its input and consumes 5 tokens.
struct EchoTool;

#[async_trait::async_trait]
impl memfuse_saos_agent::AgentTool for EchoTool {
    fn name(&self) -> &str {
        "echo_tool"
    }

    async fn execute(
        &self,
        _ctx: &AgentContext,
        input: serde_json::Value,
    ) -> memfuse_core::Result<StepResult> {
        Ok(StepResult {
            node_id: "echo".to_string(),
            output: json!({"echo": input}),
            tokens_consumed: 5,
            next_edge: None,
        })
    }
}

/// Helper: creates a MemFuse DB + state collection + OrchestratorEngine.
async fn setup_engine(dim: usize) -> (OrchestratorEngine, Arc<MemFuse>, TempDir) {
    let tmp = TempDir::new().expect("temp dir"); // expect #[cfg(test)]
    let config = MemFuseConfig {
        dimension: dim,
        max_elements: 10_000,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };
    let db = Arc::new(
        MemFuse::open_with_config(tmp.path(), config)
            .await
            .expect("open db"), // expect #[cfg(test)]
    );

    let storage = db.inner_storage();
    let mut engine = OrchestratorEngine::new(storage);
    engine.register_tool(Box::new(EchoTool));

    (engine, db, tmp)
}

#[tokio::test]
async fn test_e2e_agent_workflow() {
    let (engine, db, _tmp) = setup_engine(3).await;

    // Build a simple Start → Task → End graph
    let mut graph = StateGraph::new();
    graph.add_node(
        "start",
        "Begin workflow",
        NodeType::Start,
        Some("echo_tool"),
    );
    graph.add_node("process", "Process data", NodeType::Task, Some("echo_tool"));
    graph.add_node("done", "Finished", NodeType::End, None);

    graph.add_edge("start", "process", None, 1);
    graph.add_edge("process", "done", None, 1);

    let state_col = Arc::new(db.collection("agent-state").await.expect("collection")); // expect #[cfg(test)] // expect #[cfg(test)]
    let budget = TokenBudget::new(100, 0);
    let mut ctx = AgentContext::new("test-task-1", "start", db.clone(), state_col, budget);

    engine.run(&mut ctx, &graph).await.expect("workflow run"); // expect #[cfg(test)] // expect #[cfg(test)]

    // Engine should have terminated at "done" node
    assert_eq!(ctx.current_node, "done");
    assert_eq!(ctx.step_count, 2); // start + process = 2 steps
}

#[tokio::test]
async fn test_e2e_db_crud_roundtrip() {
    let tmp = TempDir::new().expect("temp dir"); // expect #[cfg(test)] // expect #[cfg(test)]
    let config = MemFuseConfig {
        dimension: 3,
        max_elements: 1000,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };
    let db = MemFuse::open_with_config(tmp.path(), config)
        .await
        .expect("open db"); // expect #[cfg(test)]

    // Insert
    db.insert("doc-1", &[1.0, 0.0, 0.0], Some(json!({"text": "Rust"})))
        .await
        .expect("insert"); // expect #[cfg(test)]

    // Search
    let results = db.search(&[1.0, 0.0, 0.0], 1).await.expect("search"); // expect #[cfg(test)]
    assert_eq!(results[0].id, "doc-1");

    // Update
    db.update("doc-1", &[1.0, 0.0, 0.0], Some(json!({"text": "Updated"})))
        .await
        .expect("update"); // expect #[cfg(test)]
    let doc = db.get("doc-1").await.expect("get").expect("exists"); // expect #[cfg(test)]
    assert_eq!(doc.metadata.expect("meta")["text"], "Updated"); // expect #[cfg(test)]

    // Delete
    db.delete("doc-1").await.expect("delete"); // expect #[cfg(test)]
    assert!(db.get("doc-1").await.expect("get").is_none()); // expect #[cfg(test)]

    // Collection isolation
    let col_a = db.collection("isolated-a").await.expect("col a"); // expect #[cfg(test)]
    let col_b = db.collection("isolated-b").await.expect("col b"); // expect #[cfg(test)]

    col_a
        .insert("key", &[0.1, 0.2, 0.3], Some(json!({"val": "A"})))
        .await
        .expect("ins a"); // expect #[cfg(test)]
    col_b
        .insert("key", &[0.1, 0.2, 0.3], Some(json!({"val": "B"})))
        .await
        .expect("ins b"); // expect #[cfg(test)]

    let va = col_a.get("key").await.expect("get a").expect("exists"); // expect #[cfg(test)]
    let vb = col_b.get("key").await.expect("get b").expect("exists"); // expect #[cfg(test)]
    assert_eq!(va.metadata.expect("meta")["val"], "A"); // expect #[cfg(test)]
    assert_eq!(vb.metadata.expect("meta")["val"], "B"); // expect #[cfg(test)]
}

#[tokio::test(flavor = "multi_thread")]
async fn test_stress_concurrent_agent_ops() {
    let tmp = TempDir::new().expect("temp dir"); // expect #[cfg(test)]
    let db = Arc::new(
        MemFuse::open_with_config(
            tmp.path(),
            MemFuseConfig {
                dimension: 4,
                max_elements: 10000,
                distance_metric: DistanceMetric::Cosine,
                ..Default::default()
            },
        )
        .await
        .expect("open db"), // expect #[cfg(test)]
    );

    let num_tasks = 10;
    let ops_per_task = 10;
    let mut handles = Vec::new();

    for t in 0..num_tasks {
        let db = db.clone();
        handles.push(tokio::spawn(async move {
            let col_name = format!("stress-{}", t);
            let col = db.collection(&col_name).await.expect("collection"); // expect #[cfg(test)]

            for i in 0..ops_per_task {
                let id = format!("task-{}-doc-{}", t, i);
                let vec = vec![t as f32, i as f32, (t + i) as f32, 0.0];

                col.insert(&id, &vec, Some(json!({"t": t, "i": i})))
                    .await
                    .expect("insert"); // expect #[cfg(test)]

                let res = col.search(&vec, 1).await.expect("search"); // expect #[cfg(test)]
                assert_eq!(res[0].id, id);

                col.delete(&id).await.expect("delete"); // expect #[cfg(test)]
                let doc = col.get(&id).await.expect("get"); // expect #[cfg(test)]
                assert!(doc.is_none());
            }
        }));
    }

    for h in handles {
        h.await.expect("task failed"); // expect #[cfg(test)]
    }
}
