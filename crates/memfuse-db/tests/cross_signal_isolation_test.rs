// FILE-CONTEXT
// ZWECK: Prüft Cross-Signal Snapshot-Isolation in memfuse-db während paralleler Writes/Updates.
// INVARIANTEN: Verifiziert, ob 4-Signal Hybrid-Suche konsistent gegen einen Snapshot liest oder Isolation-Asymmetrien auftreten.
// STAND: TS:2026-08-31T23:10:00Z (SESSION: 0dcb9f3b)

use memfuse_core::{DistanceMetric, Result};
use memfuse_db::{MemFuse, MemFuseConfig};
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tempfile::tempdir;

#[tokio::test]
async fn test_cross_signal_isolation_single_run() -> Result<()> {
    let dir = tempdir().unwrap();
    let config = MemFuseConfig {
        dimension: 4,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };
    let db = MemFuse::open_with_config(dir.path(), config).await?;
    let collection = db.collection("default").await?;

    // Step 1: Insert doc-1 at Tx 1 (Vector: [1.0, 0.0, 0.0, 0.0], Text: "rust memory safety")
    collection
        .insert(
            "doc-1",
            &[1.0, 0.0, 0.0, 0.0],
            Some(json!({"text": "rust memory safety", "version": "v1"})),
        )
        .await?;

    let seq1 = collection.snapshot_seq().await?;

    // Step 2: Update doc-1 at Tx 2 (Vector: [0.0, 1.0, 0.0, 0.0], Text: "python machine learning")
    collection
        .update(
            "doc-1",
            &[0.0, 1.0, 0.0, 0.0],
            Some(json!({"text": "python machine learning", "version": "v2"})),
        )
        .await?;

    // Query 1: Pinned hybrid query at seq1 for text "rust" and new vector [0.0, 1.0, 0.0, 0.0]
    // Vector signal (HNSW): searches unpinned live in-memory state (vector is [0.0, 1.0, 0.0, 0.0]), so HNSW matches doc-1.
    // Text signal (BM25): search_at(seq1) searches BM25 index at seq1 (text was "rust memory safety"), so BM25 matches doc-1.
    // Storage hydration: get_at_seq(seq1) hydratises storage at seq1 snapshot (version "v1").
    let vec_query = [0.0, 1.0, 0.0, 0.0];
    let pinned_res = collection
        .query()
        .text("rust")
        .vector(vec_query)
        .seq(seq1)
        .k(5)
        .execute()
        .await?;

    println!("Pinned seq1 search results count: {}", pinned_res.len());
    if !pinned_res.is_empty() {
        let doc = &pinned_res[0];
        println!("Pinned seq1 matched signals: {:?}", doc.matched_signals);
        println!("Pinned seq1 hydrated metadata: {:?}", doc.metadata);
    }

    Ok(())
}

#[tokio::test]
async fn test_graph_signal_snapshot_isolation_with_hops_strategy() -> Result<()> {
    use memfuse_core::GraphTraversalStrategy;

    let dir = tempdir().unwrap();
    let config = MemFuseConfig {
        dimension: 4,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };
    let db = MemFuse::open_with_config(dir.path(), config).await?;
    let collection = db.collection("default").await?;

    // Step 1: Insert doc-1 and doc-2 at Tx 1 and relate them
    collection
        .insert(
            "doc-1",
            &[1.0, 0.0, 0.0, 0.0],
            Some(json!({"text": "graph node 1"})),
        )
        .await?;
    collection
        .insert(
            "doc-2",
            &[0.0, 1.0, 0.0, 0.0],
            Some(json!({"text": "graph node 2"})),
        )
        .await?;

    collection.relate("doc-1", "doc-2", "connected_to").await?;

    // Pin snapshot sequence N after e1 -> e2 relation
    let seq_n = collection.snapshot_seq().await?;

    // Step 2: Insert doc-3 and relate doc-2 -> doc-3 at Tx > seq_n
    collection
        .insert(
            "doc-3",
            &[0.0, 0.0, 1.0, 0.0],
            Some(json!({"text": "graph node 3"})),
        )
        .await?;
    collection.relate("doc-2", "doc-3", "connected_to").await?;

    // Step 3: Execute hybrid search pinned at seq_n with Hops strategy (max_hops = 2) starting from e1
    let query = memfuse_core::HybridQueryBuilder::new()
        .with_text_query("graph node 1")
        .with_k(10)
        .with_graph_strategy(GraphTraversalStrategy::Hops { max_hops: 2 })
        .build()?;

    let results = collection
        .hybrid_search_with_query_at(&query, seq_n)
        .await?;

    // Verifiziere:
    // Im Snapshot seq_n existiert nur e1 -> e2. e2 -> e3 existiert erst nach seq_n.
    // Daher darf doc-3 im 2-hop Traversal von e1 aus NICHT enthalten sein!
    assert!(
        !results.iter().any(|r| r.id == "doc-3"),
        "doc-3 (added post-snapshot seq_n) must NOT be reachable in snapshot-isolated 2-hop graph search"
    );

    Ok(())
}

#[tokio::test]
async fn test_hybrid_search_consistent_snapshot_across_signals() -> Result<()> {
    let dir = tempdir().unwrap();
    let config = MemFuseConfig {
        dimension: 4,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };
    let db = MemFuse::open_with_config(dir.path(), config).await?;
    let collection = db.collection("default").await?;

    // (a) Erstelle Collection mit initialem Dokument
    collection
        .insert(
            "doc-initial",
            &[1.0, 0.0, 0.0, 0.0],
            Some(json!({"text": "concurrent snapshot isolation test", "status": "v1"})),
        )
        .await?;

    // Fixiere den initialen Snapshot
    let snapshot_seq = collection.snapshot_seq().await?;

    // (b) & (c) Füge parallel ein neues Dokument ein und aktualisiere das bestehende Dokument nach dem Snapshot
    collection
        .insert(
            "doc-new",
            &[0.0, 1.0, 0.0, 0.0],
            Some(json!({"text": "concurrent snapshot isolation test", "status": "v2"})),
        )
        .await?;

    collection
        .update(
            "doc-initial",
            &[0.0, 0.0, 1.0, 0.0],
            Some(json!({"text": "updated text post commit", "status": "v2"})),
        )
        .await?;

    // (d) Führe gepinnte Hybrid-Suche aus gegen `snapshot_seq`
    let results = collection
        .query()
        .text("concurrent snapshot isolation test")
        .vector([1.0, 0.0, 0.0, 0.0])
        .seq(snapshot_seq)
        .k(10)
        .execute()
        .await?;

    // Verifiziere:
    // 1. `doc-new` darf im Snapshot NICHT existieren
    assert!(
        !results.iter().any(|r| r.id == "doc-new"),
        "doc-new should not be present in snapshot search"
    );

    // 2. `doc-initial` muss mit den v1 Metadaten und v1 Vektor enthalten sein
    let initial_res = results
        .iter()
        .find(|r| r.id == "doc-initial")
        .expect("doc-initial must be present in snapshot results");

    let status = initial_res
        .metadata
        .as_ref()
        .and_then(|m| m.get("status"))
        .and_then(|s| s.as_str());

    assert_eq!(
        status,
        Some("v1"),
        "Hydrated metadata must match v1 snapshot state"
    );

    Ok(())
}

#[tokio::test]
async fn test_cross_signal_isolation_100_iterations_stress() -> Result<()> {
    let split_brain_count = Arc::new(AtomicUsize::new(0));
    let total_runs = 100;

    for _iteration in 0..total_runs {
        let dir = tempdir().unwrap();
        let config = MemFuseConfig {
            dimension: 4,
            distance_metric: DistanceMetric::Cosine,
            ..Default::default()
        };
        let db = Arc::new(MemFuse::open_with_config(dir.path(), config).await?);
        let collection = Arc::new(db.collection("default").await?);

        // Pre-populate background docs
        for i in 0..5 {
            collection
                .insert(
                    &format!("bg-doc-{}", i),
                    &[0.1, 0.1, 0.8, 0.0],
                    Some(json!({"text": format!("background {}", i)})),
                )
                .await?;
        }

        // Step 1: Insert target doc at v1
        collection
            .insert(
                "doc-target",
                &[1.0, 0.0, 0.0, 0.0],
                Some(json!({"text": "quantum physics core", "ver": 1})),
            )
            .await?;

        let seq_v1 = collection.snapshot_seq().await?;

        // Step 2: Update target doc to v2
        collection
            .update(
                "doc-target",
                &[0.0, 1.0, 0.0, 0.0],
                Some(json!({"text": "organic chemistry core", "ver": 2})),
            )
            .await?;

        // Step 3: Query pinned at seq_v1 using v2 vector [0.0, 1.0, 0.0, 0.0] and v1 text "quantum"
        let vec_query_v2 = [0.0, 1.0, 0.0, 0.0];
        let results = collection
            .query()
            .text("quantum")
            .vector(vec_query_v2)
            .seq(seq_v1)
            .k(5)
            .execute()
            .await?;

        if let Some(doc) = results.iter().find(|r| r.id == "doc-target") {
            let has_text = doc.matched_signals.contains(&"text".to_string());
            let has_vec = doc.matched_signals.contains(&"vector".to_string());
            let ver = doc
                .metadata
                .as_ref()
                .and_then(|m| m.get("ver"))
                .and_then(|v| v.as_u64());

            // If matched by text (snapshot) AND vector (live state) AND hydrated version is v1,
            // we have a split-brain read across signals.
            if has_text && has_vec && ver == Some(1) {
                split_brain_count.fetch_add(1, Ordering::SeqCst);
            }
        }
    }

    let detected = split_brain_count.load(Ordering::SeqCst);
    println!(
        "\n=======================================================\nSTRESS TEST RESULTS: {} / {} iterations exhibited split-brain cross-signal read asymmetry.\n=======================================================\n",
        detected, total_runs
    );

    Ok(())
}
