//! Regressionstest für AGT-INDEX-006: Snapshot-Isolation bei Soft-Delete.
//! Sichert: search_at(seq) gibt Nodes zurück die NACH seq gelöscht wurden.

use memfuse_core::{DocId, TxId, VectorIndex};
use memfuse_index::{HnswConfig, HnswIndex};

#[tokio::test]
async fn test_search_at_includes_node_deleted_after_snapshot() {
    let config = HnswConfig {
        dimension: 4,
        ..Default::default()
    };
    let index = HnswIndex::try_new(config).expect("valid index");

    // tx1 (seq=1): Insert doc_id=1 ([1.0, 0.0, 0.0, 0.0])
    //              Insert doc_id=2 ([0.0, 1.0, 0.0, 0.0])
    let tx1 = TxId::new(1);
    index
        .insert(tx1, DocId::new(1), &[1.0, 0.0, 0.0, 0.0])
        .await
        .expect("insert 1");
    index
        .insert(tx1, DocId::new(2), &[0.0, 1.0, 0.0, 0.0])
        .await
        .expect("insert 2");
    index.commit(tx1).await.expect("commit tx1");

    // tx2 (seq=2): Delete doc_id=1
    //              Insert doc_id=3 ([0.5, 0.5, 0.0, 0.0])
    let tx2 = TxId::new(2);
    index.delete(tx2, DocId::new(1)).await.expect("delete 1");
    index
        .insert(tx2, DocId::new(3), &[0.5, 0.5, 0.0, 0.0])
        .await
        .expect("insert 3");
    index.commit(tx2).await.expect("commit tx2");

    // search_at(query=[1,0,0,0], k=5, seq=1) MUSS doc_id=1 enthalten
    let res_seq1 = index
        .search_at(&[1.0, 0.0, 0.0, 0.0], 5, 1)
        .await
        .expect("search_at seq=1");
    let docs_seq1: Vec<_> = res_seq1.iter().map(|d| d.doc_id.inner()).collect();
    assert!(
        docs_seq1.contains(&1),
        "search_at(seq=1) MUSS doc_id=1 enthalten"
    );
    assert!(
        docs_seq1.contains(&2),
        "search_at(seq=1) MUSS doc_id=2 enthalten"
    );
    // search_at(query=[1,0,0,0], k=5, seq=1) darf doc_id=3 NICHT enthalten
    assert!(
        !docs_seq1.contains(&3),
        "search_at(seq=1) darf doc_id=3 NICHT enthalten"
    );

    // search_at(query=[1,0,0,0], k=5, seq=2) darf doc_id=1 NICHT enthalten
    let res_seq2 = index
        .search_at(&[1.0, 0.0, 0.0, 0.0], 5, 2)
        .await
        .expect("search_at seq=2");
    let docs_seq2: Vec<_> = res_seq2.iter().map(|d| d.doc_id.inner()).collect();
    assert!(
        !docs_seq2.contains(&1),
        "search_at(seq=2) darf doc_id=1 NICHT enthalten"
    );
    // search_at(query=[1,0,0,0], k=5, seq=2) MUSS doc_id=2 enthalten
    assert!(
        docs_seq2.contains(&2),
        "search_at(seq=2) MUSS doc_id=2 enthalten"
    );
    // search_at(query=[1,0,0,0], k=5, seq=2) MUSS doc_id=3 enthalten
    assert!(
        docs_seq2.contains(&3),
        "search_at(seq=2) MUSS doc_id=3 enthalten"
    );
}

#[tokio::test]
async fn test_search_at_self_reference_not_deleted_before_snapshot() {
    let config = HnswConfig {
        dimension: 4,
        ..Default::default()
    };
    let index = HnswIndex::try_new(config).expect("valid index");

    // Insert A@seq=1
    let tx1 = TxId::new(1);
    index
        .insert(tx1, DocId::new(10), &[1.0, 0.0, 0.0, 0.0])
        .await
        .expect("insert A");
    index.commit(tx1).await.expect("commit tx1");

    // Dummy tx2 (seq=2)
    let tx2 = TxId::new(2);
    index
        .insert(tx2, DocId::new(20), &[0.0, 1.0, 0.0, 0.0])
        .await
        .expect("insert B");
    index.commit(tx2).await.expect("commit tx2");

    // Delete A@seq=3
    let tx3 = TxId::new(3);
    index.delete(tx3, DocId::new(10)).await.expect("delete A");
    index.commit(tx3).await.expect("commit tx3");

    // search_at(seq=2) MUSS A enthalten (Löschung ist NACH dem Snapshot)
    let res_seq2 = index
        .search_at(&[1.0, 0.0, 0.0, 0.0], 5, 2)
        .await
        .expect("search_at seq=2");
    let docs_seq2: Vec<_> = res_seq2.iter().map(|d| d.doc_id.inner()).collect();
    assert!(
        docs_seq2.contains(&10),
        "search_at(seq=2) MUSS doc_id=10 enthalten (Löschung war erst bei seq=3)"
    );

    // search_at(seq=3) darf A NICHT enthalten
    let res_seq3 = index
        .search_at(&[1.0, 0.0, 0.0, 0.0], 5, 3)
        .await
        .expect("search_at seq=3");
    let docs_seq3: Vec<_> = res_seq3.iter().map(|d| d.doc_id.inner()).collect();
    assert!(
        !docs_seq3.contains(&10),
        "search_at(seq=3) darf doc_id=10 NICHT enthalten"
    );

    // search_at(seq=4) darf A NICHT enthalten
    let res_seq4 = index
        .search_at(&[1.0, 0.0, 0.0, 0.0], 5, 4)
        .await
        .expect("search_at seq=4");
    let docs_seq4: Vec<_> = res_seq4.iter().map(|d| d.doc_id.inner()).collect();
    assert!(
        !docs_seq4.contains(&10),
        "search_at(seq=4) darf doc_id=10 NICHT enthalten"
    );
}
