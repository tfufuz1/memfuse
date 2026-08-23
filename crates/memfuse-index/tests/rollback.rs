use memfuse_core::traits::VectorIndex;
use memfuse_core::types::{DocId, TxId};
use memfuse_index::{HnswConfig, HnswIndex};

#[tokio::test]
async fn test_hnsw_rollback_to_tx_removes_nodes() {
    let config = HnswConfig {
        dimension: 4,
        ..Default::default()
    };
    let index = HnswIndex::new(config);

    // 1. Insert doc 1 with TX=1 and commit
    let tx1 = TxId::new(1);
    let doc1 = DocId::new(100);
    index
        .insert(tx1, doc1, &[1.0, 0.0, 0.0, 0.0])
        .await
        .expect("insert tx1");
    index.commit(tx1).await.expect("commit tx1");

    // 2. Insert doc 2 with TX=2 and commit
    let tx2 = TxId::new(2);
    let doc2 = DocId::new(200);
    index
        .insert(tx2, doc2, &[0.0, 1.0, 0.0, 0.0])
        .await
        .expect("insert tx2");
    index.commit(tx2).await.expect("commit tx2");

    // Verify both documents are searchable
    let res_before = index
        .search(&[1.0, 0.0, 0.0, 0.0], 10)
        .await
        .expect("search before rollback");
    assert_eq!(res_before.len(), 2);

    // 3. Rollback to TX=1
    index.rollback_to_tx(tx1).await.expect("rollback_to_tx");

    // 4. Verify: Doc 2 (from TX=2) is no longer reachable via search()
    // and Doc 1 (from TX=1) is still present.
    let res_after1 = index
        .search(&[0.0, 1.0, 0.0, 0.0], 10)
        .await
        .expect("search doc2 after rollback");
    assert!(
        res_after1.iter().all(|d| d.doc_id != doc2),
        "Doc 2 from TX=2 should not be reachable after rollback to TX=1"
    );

    let res_after2 = index
        .search(&[1.0, 0.0, 0.0, 0.0], 10)
        .await
        .expect("search doc1 after rollback");
    assert!(
        res_after2.iter().any(|d| d.doc_id == doc1),
        "Doc 1 from TX=1 should still be present after rollback to TX=1"
    );

    // Verify length of index
    assert_eq!(index.len().await, 1);
}
