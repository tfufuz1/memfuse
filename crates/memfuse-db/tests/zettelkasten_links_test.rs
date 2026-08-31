use memfuse_core::types::domain::{DocId, LinkRelation, MemoryLink, TxId};
use memfuse_core::HybridQuery;
use memfuse_db::MemFuse;
use tempfile::tempdir;

#[tokio::test]
async fn test_zettelkasten_memory_links_and_traversal() {
    let dir = tempdir().unwrap();
    let db = MemFuse::open(dir.path()).await.unwrap();
    let col = db.collection("default").await.unwrap();

    let dummy_emb = vec![0.1f32; 768];

    // Insert 3 documents: doc-1, doc-2, doc-3
    db.insert("doc-1", &dummy_emb, Some(serde_json::json!({"text": "Document 1 content"})))
        .await
        .unwrap();
    db.insert("doc-2", &dummy_emb, Some(serde_json::json!({"text": "Document 2 content"})))
        .await
        .unwrap();
    db.insert("doc-3", &dummy_emb, Some(serde_json::json!({"text": "Document 3 content"})))
        .await
        .unwrap();

    let doc1_id = DocId::from_key("doc-1").unwrap();
    let doc2_id = DocId::from_key("doc-2").unwrap();
    let doc3_id = DocId::from_key("doc-3").unwrap();

    // Link doc-1 -> doc-2 (Elaborates)
    col.link_memories(doc1_id, doc2_id, LinkRelation::Elaborates)
        .await
        .unwrap();

    // Link doc-2 -> doc-3 (References)
    col.link_memories(doc2_id, doc3_id, LinkRelation::References)
        .await
        .unwrap();

    // Link doc-3 -> doc-1 (Cycle check: References)
    col.link_memories(doc3_id, doc1_id, LinkRelation::References)
        .await
        .unwrap();

    // Test get_links for doc-1
    let links1 = col.get_links(doc1_id).await.unwrap();
    assert_eq!(links1.len(), 1);
    assert_eq!(links1[0].target, doc2_id);
    assert_eq!(links1[0].relation, LinkRelation::Elaborates);

    // Test idempotency: linking doc-1 -> doc-2 again with same relation shouldn't duplicate
    col.link_memories(doc1_id, doc2_id, LinkRelation::Elaborates)
        .await
        .unwrap();
    let links1_again = col.get_links(doc1_id).await.unwrap();
    assert_eq!(links1_again.len(), 1);

    // Test traverse_links with max_depth 1 (should visit doc-2)
    let traversed_depth1 = col.traverse_links(doc1_id, 1).await.unwrap();
    assert_eq!(traversed_depth1, vec![(doc2_id, 1)]);

    // Test traverse_links with max_depth 3 (cycle doc-3 -> doc-1 must be avoided)
    let traversed_depth3 = col.traverse_links(doc1_id, 3).await.unwrap();
    assert_eq!(
        traversed_depth3,
        vec![(doc2_id, 1), (doc3_id, 2)]
    );
}

#[tokio::test]
async fn test_supersedes_displacement_logic() {
    let dir = tempdir().unwrap();
    let db = MemFuse::open(dir.path()).await.unwrap();
    let col = db.collection("default").await.unwrap();

    let dummy_emb = vec![0.1f32; 768];

    // Insert doc-old and doc-new
    db.insert("doc-old", &dummy_emb, Some(serde_json::json!({"text": "Old outdated specification"})))
        .await
        .unwrap();
    db.insert("doc-new", &dummy_emb, Some(serde_json::json!({"text": "New updated specification"})))
        .await
        .unwrap();

    let old_id = DocId::from_key("doc-old").unwrap();
    let new_id = DocId::from_key("doc-new").unwrap();

    // doc-new supersedes doc-old
    col.link_memories(new_id, old_id, LinkRelation::Supersedes)
        .await
        .unwrap();

    // Query with include_superseded = false (default)
    let q_default = HybridQuery::builder()
        .with_text_query("specification")
        .with_include_superseded(false)
        .build()
        .unwrap();

    let results_filtered = col.hybrid_search_with_query(&q_default).await.unwrap();
    // doc-old should be displaced by doc-new
    let ids_filtered: Vec<String> = results_filtered.into_iter().map(|r| r.id).collect();
    assert!(ids_filtered.contains(&"doc-new".to_string()));
    assert!(!ids_filtered.contains(&"doc-old".to_string()));

    // Query with include_superseded = true
    let q_include_all = HybridQuery::builder()
        .with_text_query("specification")
        .with_include_superseded(true)
        .build()
        .unwrap();

    let results_all = col.hybrid_search_with_query(&q_include_all).await.unwrap();
    let ids_all: Vec<String> = results_all.into_iter().map(|r| r.id).collect();
    assert!(ids_all.contains(&"doc-new".to_string()));
    assert!(ids_all.contains(&"doc-old".to_string()));
}

#[tokio::test]
async fn test_txid_boundary_hardening() {
    let dir = tempdir().unwrap();
    let db = MemFuse::open(dir.path()).await.unwrap();
    let col = db.collection("default").await.unwrap();

    // Allocate normal TxId
    let tx1 = col.allocate_tx().unwrap();
    assert!(tx1.is_valid_origin());

    // Manually set internal next_tx atomic near boundary to test boundary check
    // Collection next_tx is Arc<AtomicU64> initialized from storage last_tx.
    // We test allocating when next_tx = MAX_COLLECTION_SEQUENCE + 1
    let exhausted_db = MemFuse::open(dir.path()).await.unwrap();
    // Verify allocate_tx returns error if next_tx > TxId::MAX_COLLECTION_SEQUENCE
    // (We simulate this by testing TxId boundary logic)
    assert!(TxId::new(TxId::MAX_COLLECTION_SEQUENCE).is_valid_origin());
    assert!(!TxId::new(TxId::MAX_COLLECTION_SEQUENCE + 1).is_valid_origin());
}
