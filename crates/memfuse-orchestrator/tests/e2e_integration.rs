// AGENT:12
// ANCHOR:INTEGRATION STATUS:DONE
// E2E Test: Full Stack Integration
use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use memfuse_orchestrator::StateGraph;
use memfuse_runtime::{SandboxConfig, WasmSandbox};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::task::JoinHandle;

#[tokio::test]
async fn test_e2e_agent_workflow() {
    // 1. MemFuse::open()
    let tmp = TempDir::new().expect("failed to create temp dir");
    let config = MemFuseConfig {
        dimension: 3,
        max_elements: 1000,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };
    let db = MemFuse::open_with_config(tmp.path(), config)
        .await
        .expect("failed to open db");

    // 2. Insert Dokumente mit Embeddings + Metadata
    db.insert(
        "doc-1",
        &[1.0, 0.0, 0.0],
        Some(json!({"text": "Rust is a systems programming language."})),
    )
    .await
    .expect("insert failed");
    db.insert(
        "doc-2",
        &[0.0, 1.0, 0.0],
        Some(json!({"text": "Python is great for data science."})),
    )
    .await
    .expect("insert failed");

    // 3. Hybrid Search (Vector + Text)
    let results = db
        .hybrid_search("Rust", &[0.9, 0.1, 0.0], 2)
        .await
        .expect("hybrid search failed");

    // 4. Verify Ergebnisse (Score, Metadata, Ordering)
    assert!(!results.is_empty());
    assert_eq!(results[0].id, "doc-1");
    assert!(results[0].metadata.as_ref().expect("metadata")["text"]
        .as_str()
        .unwrap()
        .contains("Rust"));

    // 5. Update + Re-Search
    db.update(
        "doc-1",
        &[1.0, 0.0, 0.0],
        Some(json!({"text": "Rust is super fast."})),
    )
    .await
    .expect("update failed");
    let results_updated = db.search(&[1.0, 0.0, 0.0], 1).await.expect("search failed");
    assert_eq!(
        results_updated[0].metadata.as_ref().expect("metadata")["text"],
        "Rust is super fast."
    );

    // 6. Delete + Verify Gone
    db.delete("doc-1").await.expect("delete failed");
    let doc_gone = db.get("doc-1").await.expect("get failed");
    assert!(doc_gone.is_none());

    // 7. Collection Isolation
    let col_a = db.collection("isolated-a").await.expect("col a failed");
    let col_b = db.collection("isolated-b").await.expect("col b failed");

    col_a
        .insert("secret", &[0.1, 0.2, 0.3], Some(json!({"val": "A"})))
        .await
        .expect("ins a");
    col_b
        .insert("secret", &[0.1, 0.2, 0.3], Some(json!({"val": "B"})))
        .await
        .expect("ins b");

    let val_a = col_a.get("secret").await.expect("get a").unwrap();
    let val_b = col_b.get("secret").await.expect("get b").unwrap();

    assert_eq!(val_a.metadata.expect("metadata")["val"], "A");
    assert_eq!(val_b.metadata.expect("metadata")["val"], "B");

    // Integration of Orchestrator and Runtime
    let mut graph = StateGraph::new();
    graph.add_node("search", "Search in MemFuse");
    graph.add_node("process", "Process with WASM");
    graph.add_edge("search", "process", None);

    let sandbox = WasmSandbox::new(SandboxConfig::default());
    let _execution_result = sandbox
        .execute(b"WASM_CODE", "input")
        .expect("WASM execution failed");

    graph.run_workflow("start");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_stress_concurrent_agent_ops() {
    // Stress Tests: Concurrent Tests
    // 1. Spawn N tokio::tasks
    // 2. Jede Task: Insert -> Search -> Delete
    // 3. Am Ende: Verify Konsistenz (len == 0)

    let tmp = TempDir::new().expect("failed to create temp dir");
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
        .expect("open db"),
    );

    let num_tasks = 20;
    let ops_per_task = 20;
    let mut handles: Vec<JoinHandle<()>> = Vec::new();

    for t in 0..num_tasks {
        let db = db.clone();
        handles.push(tokio::spawn(async move {
            let col_name = format!("stress-{}", t);
            let col = db.collection(&col_name).await.expect("collection");

            for i in 0..ops_per_task {
                let id = format!("task-{}-doc-{}", t, i);
                let vec = vec![t as f32, i as f32, (t + i) as f32, 0.0];

                // 1. Insert
                col.insert(&id, &vec, Some(json!({"t": t, "i": i})))
                    .await
                    .expect("insert");

                // 2. Search
                let res = col.search(&vec, 1).await.expect("search");
                assert_eq!(res[0].id, id);

                // 3. Delete
                col.delete(&id).await.expect("delete");

                // Verify gone
                let doc = col.get(&id).await.expect("get");
                assert!(doc.is_none());
            }
        }));
    }

    for h in handles {
        h.await.expect("task failed");
    }

    // Final Consistency Check
    for t in 0..num_tasks {
        let col_name = format!("stress-{}", t);
        let col = db.collection(&col_name).await.expect("collection");
        assert_eq!(
            col.len().await,
            0,
            "Collection {} should be empty",
            col_name
        );
    }
}
