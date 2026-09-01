//! Regressionstest für AGT-INDEX-006: Snapshot-Isolation bei Soft-Delete.
//! Sichert: search_at(seq) gibt Nodes zurück die NACH seq gelöscht wurden.

use memfuse_core::{DocId, TxId, VectorIndex};
use memfuse_index::{HnswConfig, HnswIndex};

#[tokio::test]
async fn test_search_at_includes_node_deleted_after_snapshot() {
    // Szenario:
    //   tx1 (seq=1): Insert doc_id=1 ([1.0, 0.0, 0.0, 0.0])
    //                Insert doc_id=2 ([0.0, 1.0, 0.0, 0.0])
    //                commit(tx1)
    //   tx2 (seq=2): Delete doc_id=1
    //                Insert doc_id=3 ([0.5, 0.5, 0.0, 0.0])
    //                commit(tx2)
    //
    // Assertions:
    //   search_at(query=[1,0,0,0], k=5, seq=1) MUSS doc_id=1 enthalten
    //   search_at(query=[1,0,0,0], k=5, seq=1) darf doc_id=3 NICHT enthalten
    //   search_at(query=[1,0,0,0], k=5, seq=2) darf doc_id=1 NICHT enthalten
    //   search_at(query=[1,0,0,0], k=5, seq=2) MUSS doc_id=2 enthalten
    //   search_at(query=[1,0,0,0], k=5, seq=2) MUSS doc_id=3 enthalten

    let config = HnswConfig {
        dimension: 4,
        ..HnswConfig::default()
    };
    let index = HnswIndex::try_new(config).expect("valid config");

    let tx1 = TxId::new(1);
    index
        .insert(tx1, DocId::new(1), &[1.0, 0.0, 0.0, 0.0])
        .await
        .unwrap();
    index
        .insert(tx1, DocId::new(2), &[0.0, 1.0, 0.0, 0.0])
        .await
        .unwrap();
    index.commit(tx1).await.unwrap();

    let tx2 = TxId::new(2);
    index.delete(tx2, DocId::new(1)).await.unwrap();
    index
        .insert(tx2, DocId::new(3), &[0.5, 0.5, 0.0, 0.0])
        .await
        .unwrap();
    index.commit(tx2).await.unwrap();

    let res_seq1 = index.search_at(&[1.0, 0.0, 0.0, 0.0], 5, 1).await.unwrap();
    let docs_seq1: Vec<_> = res_seq1.iter().map(|d| d.doc_id.inner()).collect();
    assert!(
        docs_seq1.contains(&1),
        "search_at(seq=1) MUSS doc_id=1 enthalten"
    );
    assert!(
        docs_seq1.contains(&2),
        "search_at(seq=1) MUSS doc_id=2 enthalten"
    );
    assert!(
        !docs_seq1.contains(&3),
        "search_at(seq=1) darf doc_id=3 NICHT enthalten"
    );

    let res_seq2 = index.search_at(&[1.0, 0.0, 0.0, 0.0], 5, 2).await.unwrap();
    let docs_seq2: Vec<_> = res_seq2.iter().map(|d| d.doc_id.inner()).collect();
    assert!(
        !docs_seq2.contains(&1),
        "search_at(seq=2) darf doc_id=1 NICHT enthalten"
    );
    assert!(
        docs_seq2.contains(&2),
        "search_at(seq=2) MUSS doc_id=2 enthalten"
    );
    assert!(
        docs_seq2.contains(&3),
        "search_at(seq=2) MUSS doc_id=3 enthalten"
    );
}

#[tokio::test]
async fn test_search_at_self_reference_not_deleted_before_snapshot() {
    // Szenario: Insert A@seq=1, Delete A@seq=3
    // search_at(seq=2) MUSS A enthalten (Löschung ist NACH dem Snapshot)
    // search_at(seq=3) darf A NICHT enthalten
    // search_at(seq=4) darf A NICHT enthalten

    let config = HnswConfig {
        dimension: 4,
        ..HnswConfig::default()
    };
    let index = HnswIndex::try_new(config).expect("valid config");

    // seq=1: Insert A (DocId 10)
    let tx1 = TxId::new(1);
    index
        .insert(tx1, DocId::new(10), &[1.0, 0.0, 0.0, 0.0])
        .await
        .unwrap();
    index.commit(tx1).await.unwrap();

    // seq=2: Insert dummy doc (DocId 20)
    let tx2 = TxId::new(2);
    index
        .insert(tx2, DocId::new(20), &[0.0, 1.0, 0.0, 0.0])
        .await
        .unwrap();
    index.commit(tx2).await.unwrap();

    // seq=3: Delete A (DocId 10)
    let tx3 = TxId::new(3);
    index.delete(tx3, DocId::new(10)).await.unwrap();
    index.commit(tx3).await.unwrap();

    // seq=4: Insert dummy doc (DocId 30)
    let tx4 = TxId::new(4);
    index
        .insert(tx4, DocId::new(30), &[0.0, 0.0, 1.0, 0.0])
        .await
        .unwrap();
    index.commit(tx4).await.unwrap();

    // search_at(seq=2) MUSS A (10) enthalten
    let res_seq2 = index.search_at(&[1.0, 0.0, 0.0, 0.0], 5, 2).await.unwrap();
    let docs_seq2: Vec<_> = res_seq2.iter().map(|d| d.doc_id.inner()).collect();
    assert!(
        docs_seq2.contains(&10),
        "search_at(seq=2) MUSS doc_id=10 enthalten"
    );

    // search_at(seq=3) darf A (10) NICHT enthalten
    let res_seq3 = index.search_at(&[1.0, 0.0, 0.0, 0.0], 5, 3).await.unwrap();
    let docs_seq3: Vec<_> = res_seq3.iter().map(|d| d.doc_id.inner()).collect();
    assert!(
        !docs_seq3.contains(&10),
        "search_at(seq=3) darf doc_id=10 NICHT enthalten"
    );

    // search_at(seq=4) darf A (10) NICHT enthalten
    let res_seq4 = index.search_at(&[1.0, 0.0, 0.0, 0.0], 5, 4).await.unwrap();
    let docs_seq4: Vec<_> = res_seq4.iter().map(|d| d.doc_id.inner()).collect();
    assert!(
        !docs_seq4.contains(&10),
        "search_at(seq=4) darf doc_id=10 NICHT enthalten"
    );
}
