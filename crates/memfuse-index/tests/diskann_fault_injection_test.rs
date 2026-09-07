// FILE-CONTEXT
// ZWECK: Fault-Injection Test for DiskANN rebuild failures (Pflichttest 1).
// INVARIANTEN: Simulates interrupted/failed DiskANN rebuild mid-write and verifies that the previous functional index state remains valid, loadable, and searchable.

#![cfg(feature = "experimental-diskann")]

use memfuse_core::{DistanceMetric, DocId, VectorIndex};
use memfuse_index::diskann::{DiskAnnConfig, DiskAnnIndex};

#[tokio::test]
async fn test_diskann_rebuild_fault_injection_preserves_previous_index() {
    let temp_dir = tempfile::tempdir().expect("tempdir creation");
    let index_path = temp_dir.path().join("fault_injection.idx");

    let config = DiskAnnConfig {
        index_path: index_path.clone(),
        dimension: 4,
        max_degree: 4,
        beam_width: 4,
        distance_metric: DistanceMetric::Euclidean,
        ..DiskAnnConfig::default()
    };

    // 1. Build initial functional V1/V2 DiskANN index with 5 vectors
    let initial_index = DiskAnnIndex::try_new(config.clone()).expect("initial DiskAnnIndex");
    let initial_vectors: Vec<Vec<f32>> = (0..5).map(|i| vec![i as f32, 0.0, 0.0, 0.0]).collect();
    let initial_ids: Vec<DocId> = (0..5).map(|i| DocId::from(100 + i as u64)).collect();

    initial_index
        .build(&initial_vectors, &initial_ids)
        .await
        .expect("initial build succeeds");

    // Verify initial search works
    let initial_query = vec![2.0f32, 0.0, 0.0, 0.0];
    let initial_res = initial_index
        .search(&initial_query, 1)
        .await
        .expect("initial search succeeds");
    assert_eq!(initial_res.len(), 1);
    assert_eq!(initial_res[0].doc_id, DocId::from(102));

    // 2. Simulate interrupted rebuild by creating a partial/corrupt `.tmp` file and failing write
    let tmp_path = index_path.with_extension("idx.tmp");
    tokio::fs::write(&tmp_path, b"PARTIAL_INTERRUPTED_WRITE_BYTES_BEFORE_CRASH")
        .await
        .expect("write partial tmp file");

    // Also attempt an interrupted task simulate
    let interrupted_index =
        DiskAnnIndex::try_new(config.clone()).expect("interrupted DiskAnnIndex");
    let mut large_vectors = initial_vectors.clone();
    large_vectors.push(vec![99.0f32, 0.0, 0.0, 0.0]);
    let mut large_ids = initial_ids.clone();
    large_ids.push(DocId::from(999));

    // Spawn build in a task and abort it mid-air to simulate abrupt process kill / crash
    let build_task =
        tokio::spawn(async move { interrupted_index.build(&large_vectors, &large_ids).await });
    // Abort immediately
    build_task.abort();
    let _ = build_task.await;

    // 3. Reload index from original path and verify the PREVIOUS valid index state remains untouched and functional
    let reloaded_index = DiskAnnIndex::try_new(config.clone()).expect("reloaded DiskAnnIndex");
    reloaded_index
        .load()
        .await
        .expect("reloading previous valid index must succeed");

    let reloaded_res = reloaded_index
        .search(&initial_query, 1)
        .await
        .expect("search on reloaded previous index must succeed");
    assert_eq!(reloaded_res.len(), 1);
    assert_eq!(
        reloaded_res[0].doc_id,
        DocId::from(102),
        "Previous index state must remain searchable and deliver original doc_id=102"
    );

    let all_ids = reloaded_index.all_doc_ids().await.expect("all_doc_ids");
    assert_eq!(all_ids.len(), 5);
    assert_eq!(all_ids, initial_ids);
}
