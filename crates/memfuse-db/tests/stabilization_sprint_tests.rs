use memfuse_core::FusionWeights;
use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn test_allocate_tx_concurrent_monotonicity() {
    let tmp = TempDir::new().expect("temp dir");
    let db = Arc::new(MemFuse::open(tmp.path()).await.expect("open db"));
    let col = Arc::new(db.collection("tx-test").await.expect("col"));

    let num_tasks = 20;
    let allocations_per_task = 100;
    let mut handles = Vec::new();

    for _ in 0..num_tasks {
        let col_clone = col.clone();
        let handle = tokio::spawn(async move {
            let mut allocated = Vec::with_capacity(allocations_per_task);
            for _ in 0..allocations_per_task {
                allocated.push(col_clone.allocate_tx().inner());
            }
            allocated
        });
        handles.push(handle);
    }

    let mut all_txs = Vec::new();
    for handle in handles {
        let txs = handle.await.expect("task join");
        all_txs.extend(txs);
    }

    assert_eq!(all_txs.len(), num_tasks * allocations_per_task);
    all_txs.sort_unstable();
    let original_len = all_txs.len();
    all_txs.dedup();
    assert_eq!(all_txs.len(), original_len, "All allocated TxIds must be strictly unique!");
}

#[tokio::test]
async fn test_concurrent_inserts_toctou_safety() {
    let tmp = TempDir::new().expect("temp dir");
    let db = Arc::new(MemFuse::open_with_config(tmp.path(), MemFuseConfig {
        dimension: 4,
        ..Default::default()
    }).await.expect("open db"));

    let col = Arc::new(db.collection("toctou-test").await.expect("col"));

    let barrier = Arc::new(tokio::sync::Barrier::new(5));
    let mut handles = Vec::new();

    for i in 0..5 {
        let col_clone = col.clone();
        let b = barrier.clone();
        handles.push(tokio::spawn(async move {
            b.wait().await;
            let doc_id = format!("same_key_{}", i);
            col_clone.insert(&doc_id, &[1.0, 0.0, 0.0, 0.0], Some(json!({"text": "concurrent test"}))).await
        }));
    }

    for handle in handles {
        let res = handle.await.expect("join task");
        assert!(res.is_ok(), "Concurrent inserts with distinct string keys should succeed without TOCTOU race");
    }
}

#[tokio::test]
async fn test_hybrid_search_uses_all_signals_and_weights() {
    let tmp = TempDir::new().expect("temp dir");
    let config = MemFuseConfig {
        dimension: 4,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };

    let db = MemFuse::open_with_config(tmp.path(), config)
        .await
        .expect("open db");
    let col = db.collection("fusion-4signal-test").await.expect("col");

    // Insert doc_vector: matches vector query
    col.insert(
        "doc_vector",
        &[1.0, 0.0, 0.0, 0.0],
        Some(json!({"text": "unrelated content AAA"})),
    )
    .await
    .unwrap();

    // Insert doc_text: matches text query
    col.insert(
        "doc_text",
        &[0.0, 1.0, 0.0, 0.0],
        Some(json!({"text": "quantum computing research"})),
    )
    .await
    .unwrap();

    // Insert doc_graph: connected to doc_text via graph relate
    col.insert(
        "doc_graph",
        &[0.0, 0.0, 1.0, 0.0],
        Some(json!({"text": "unrelated content BBB"})),
    )
    .await
    .unwrap();

    col.relate("doc_text", "doc_graph", "cites").await.unwrap();

    let text_query = "quantum computing";
    let vector_query = [1.0, 0.0, 0.0, 0.0];

    // Hybrid search with custom fusion weights
    let weights = FusionWeights::new(0.5, 0.3, 0.2).unwrap();
    let results = col
        .hybrid_search_with_weights(
            text_query,
            &vector_query,
            10,
            None,
            Some(&weights),
        )
        .await
        .unwrap();

    assert!(!results.is_empty());
    // Verify that matched_signals track the signal sources
    let vector_doc = results.iter().find(|r| r.id == "doc_vector").unwrap();
    assert!(vector_doc.matched_signals.contains(&"vector".to_string()));

    let text_doc = results.iter().find(|r| r.id == "doc_text").unwrap();
    assert!(text_doc.matched_signals.contains(&"text".to_string()));

    let graph_doc = results.iter().find(|r| r.id == "doc_graph").unwrap();
    assert!(graph_doc.matched_signals.contains(&"graph".to_string()));
}
