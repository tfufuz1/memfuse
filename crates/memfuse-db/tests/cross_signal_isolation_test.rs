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
    let pinned_res = collection
        .query()
        .text("rust")
        .vector([0.0, 1.0, 0.0, 0.0])
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
        let results = collection
            .query()
            .text("quantum")
            .vector([0.0, 1.0, 0.0, 0.0])
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
