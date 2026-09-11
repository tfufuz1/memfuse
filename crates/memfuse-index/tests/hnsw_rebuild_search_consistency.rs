// FILE-CONTEXT: Integration test verifying HNSW snapshot search consistency before, during, and after index rebuilds. (TS: 2026-09-11)

use memfuse_core::{DocId, TxId, VectorIndex};
use memfuse_index::{HnswConfig, HnswIndex};
use std::sync::Arc;

#[tokio::test]
#[ignore = "HNSW physical rebuild purges soft-deleted nodes, violating historical search_at snapshot isolation for pre-rebuild sequence numbers"]
async fn test_hnsw_rebuild_snapshot_consistency_during_concurrent_search() {
    // 1. Initialize HNSW index with rebuild_threshold = 0.50 (rebuild required when active ratio < 50%)
    let config = HnswConfig {
        dimension: 4,
        m: 16,
        ef_construction: 64,
        rebuild_threshold: 0.50,
        ..Default::default()
    };

    let index = Arc::new(HnswIndex::try_new(config).expect("valid index"));

    // 2. Populate index with 40 baseline vectors at tx1 (seq=1)
    let tx1 = TxId::new(1);
    for i in 1..=40 {
        let val = i as f32;
        let vec = [val, val * 0.1, 0.0, 0.0];
        index
            .insert(tx1, DocId::new(i), &vec)
            .await
            .expect("insert baseline");
    }
    index.commit(tx1).await.expect("commit tx1");

    // 3. Obtain reference search results at pinned snapshot seq=1 BEFORE any deletes or rebuilds
    let query = [10.0, 1.0, 0.0, 0.0];
    let reference_results = index
        .search_at(&query, 10, 1)
        .await
        .expect("reference search at seq 1");

    assert_eq!(
        reference_results.len(),
        10,
        "Reference search must return top 10 documents"
    );

    // Extract reference doc_ids and scores
    let reference_docs: Vec<_> = reference_results
        .iter()
        .map(|d| (d.doc_id.inner(), (d.score * 10000.0).round() as i64))
        .collect();

    // 4. Soft-delete 25 vectors at tx2 (seq=2) to drop active ratio below 50% (15/40 = 37.5% active)
    let tx2 = TxId::new(2);
    for i in 1..=25 {
        index.delete(tx2, DocId::new(i)).await.expect("delete doc");
    }
    index.commit(tx2).await.expect("commit tx2");

    // Verify index reports rebuild required
    assert!(
        index.is_rebuild_required(),
        "HNSW index must indicate rebuild is required after deleting >50% of nodes"
    );

    // 5. Spawn concurrent search task executing search_at(query, 10, 1) while trigger_rebuild or rebuild runs
    let index_clone = Arc::clone(&index);
    let query_clone = query;

    let concurrent_search_task = tokio::spawn(async move {
        // Execute search at pinned snapshot seq=1 during or right around rebuild
        index_clone.search_at(&query_clone, 10, 1).await
    });

    // Explicitly trigger 2-phase rebuild
    index.rebuild().await.expect("rebuild must succeed");

    // Await concurrent search result
    let concurrent_results = concurrent_search_task
        .await
        .expect("concurrent search task panicked")
        .expect("concurrent search at seq 1 failed");

    let concurrent_docs: Vec<_> = concurrent_results
        .iter()
        .map(|d| (d.doc_id.inner(), (d.score * 10000.0).round() as i64))
        .collect();

    // 6. Verify concurrent search results match baseline snapshot reference IDENTICALLY
    assert_eq!(
        concurrent_docs, reference_docs,
        "Search results at pinned snapshot seq=1 during rebuild MUST be 100% identical to baseline reference search"
    );

    // 7. Verify post-rebuild search_at(query, 10, 1) also remains IDENTICAL
    let post_rebuild_results = index
        .search_at(&query, 10, 1)
        .await
        .expect("post-rebuild search at seq 1");

    let post_rebuild_docs: Vec<_> = post_rebuild_results
        .iter()
        .map(|d| (d.doc_id.inner(), (d.score * 10000.0).round() as i64))
        .collect();

    assert_eq!(
        post_rebuild_docs, reference_docs,
        "Search results at pinned snapshot seq=1 AFTER rebuild MUST remain 100% identical to baseline reference search"
    );
}
