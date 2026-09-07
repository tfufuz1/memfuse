// FILE-CONTEXT
// ZWECK: Collection Integration Fallback Test (Pflichttest 3).
// INVARIANTEN: Proves that Collection::<S, DiskAnnIndex>::query() continues returning search results (via HNSW fallback) after a simulated DiskANN rebuild failure or corruption, rather than returning Err to the caller.

#![cfg(feature = "experimental-diskann")]

use memfuse_core::{DistanceMetric, DocId, VectorIndex};
use memfuse_db::Collection;
use memfuse_graph::csr::CsrGraph;
use memfuse_index::{DiskAnnConfig, DiskAnnFallbackPolicy, DiskAnnIndex};
use memfuse_store::LsmStorage;
use memfuse_text::Language;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

#[tokio::test]
async fn test_collection_query_with_corrupted_diskann_returns_results_via_fallback() {
    let dir = tempfile::tempdir().expect("tempdir");
    let diskann_path = dir.path().join("collection_diskann.idx");

    let diskann_config = DiskAnnConfig {
        index_path: diskann_path.clone(),
        dimension: 4,
        max_degree: 4,
        beam_width: 4,
        distance_metric: DistanceMetric::Euclidean,
        fallback_policy: DiskAnnFallbackPolicy::UseHnswOnFailure,
        ..DiskAnnConfig::default()
    };

    // 1. Build initial DiskANN index with 5 vectors
    let diskann = Arc::new(DiskAnnIndex::try_new(diskann_config.clone()).expect("try_new"));
    let vectors: Vec<Vec<f32>> = (0..5).map(|i| vec![i as f32, 0.0, 0.0, 0.0]).collect();
    let ids: Vec<DocId> = (0..5).map(|i| DocId::from(100 + i as u64)).collect();
    diskann.build(&vectors, &ids).await.expect("build succeeds");

    // 2. Intentionally corrupt the index file on disk
    let mut data = tokio::fs::read(&diskann_path).await.expect("read file");
    data[12] ^= 0xFF; // Corrupt payload byte
    tokio::fs::write(&diskann_path, &data)
        .await
        .expect("write corrupt file");

    // 3. Create a reloaded DiskANN index instance that fails loading and falls back to HNSW
    let reloaded_diskann =
        Arc::new(DiskAnnIndex::try_new(diskann_config).expect("try_new reloaded"));
    let _ = reloaded_diskann.load().await; // Load fails & activates HNSW fallback internally

    // Insert new document into the fallback index
    let tx = memfuse_core::TxId::new(10);
    reloaded_diskann
        .insert(tx, DocId::from(500), &[1.0f32, 0.0, 0.0, 0.0])
        .await
        .expect("insert into fallback");
    reloaded_diskann.commit(tx).await.expect("commit fallback");

    // 4. Initialize Storage and Collection with the fallback-enabled DiskANN vector index
    let storage = Arc::new(
        LsmStorage::new(memfuse_store::LsmConfig {
            path: dir.path().join("lsm"),
            ..Default::default()
        })
        .await
        .expect("open storage"),
    );
    let graph = Arc::new(CsrGraph::new());
    let next_tx = Arc::new(AtomicU64::new(1));

    let collection = Collection::<LsmStorage, DiskAnnIndex>::new(
        "test_diskann_fallback_col".to_string(),
        storage.clone(),
        reloaded_diskann,
        graph,
        next_tx,
        4,
        Language::English,
    );

    // Insert document through Collection API
    collection
        .insert("doc_500", &[1.0f32, 0.0, 0.0, 0.0], None)
        .await
        .expect("insert through collection");

    // 5. Query collection via regular Collection::query() path
    let search_results = collection
        .query()
        .vector(vec![1.0f32, 0.0, 0.0, 0.0])
        .k(5)
        .execute()
        .await;

    // Verify Collection::query() does NOT return an Err, but returns valid results
    assert!(
        search_results.is_ok(),
        "Collection::query() must return Ok(...) even when DiskANN vector index is in fallback mode"
    );
    let docs = search_results.unwrap();
    assert!(
        !docs.is_empty(),
        "Query results must not be empty (should contain doc_id 500 from fallback HNSW)"
    );
}
