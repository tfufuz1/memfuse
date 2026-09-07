#![allow(deprecated)]

use memfuse_core::types::domain::{DocId, EntityId, LinkRelation};
use memfuse_graph::PathRAGEngine;
use memfuse_db::MemFuse;
use tempfile::tempdir;

#[tokio::test]
async fn test_supersedes_triggers_edge_tombstone() {
    let dir = tempdir().unwrap();
    let db = MemFuse::open(dir.path()).await.unwrap();
    let col = db.collection("default").await.unwrap();

    let dummy_emb = vec![0.1f32; 768];

    // 1. Zwei Dokumente mit Graph-Kante zwischen ihnen anlegen
    db.insert(
        "doc-a",
        &dummy_emb,
        Some(serde_json::json!({"text": "Document A text"})),
    )
    .await
    .unwrap();

    db.insert(
        "doc-b",
        &dummy_emb,
        Some(serde_json::json!({"text": "Document B text"})),
    )
    .await
    .unwrap();

    let doc_a_id = DocId::from_key("doc-a").unwrap();
    let doc_b_id = DocId::from_key("doc-b").unwrap();

    col.relate("doc-a", "doc-b", "relates_to").await.unwrap();

    let eid_a = EntityId::from_key("doc-a").unwrap();
    let eid_b = EntityId::from_key("doc-b").unwrap();
    let edge_id = (eid_a, eid_b);

    // 2. Kante wird von relate() automatisch in doc_edge_index indiziert (INV-GRAPH-PROV-1)
    let recorded_edges = col.graph_index().doc_edge_index.edges_for_doc(doc_a_id);
    assert!(
        recorded_edges.contains(&edge_id),
        "Automatic edge registration in doc_edge_index failed"
    );

    // Verify PathRAG initially finds path
    let engine = PathRAGEngine::with_defaults(col.graph_index());
    let path_before = engine.find_path(eid_a, eid_b);
    assert!(path_before.is_some(), "Path should exist before superseding doc_a");

    // 3. doc_b supersedes doc_a
    col.link_memories(doc_b_id, doc_a_id, LinkRelation::Supersedes)
        .await
        .unwrap();

    // 4. Assertion: Kante ist tombstoniert & PathRAG find_path() findet die Kante nicht mehr
    let path_after = engine.find_path(eid_a, eid_b);
    assert!(
        path_after.is_none(),
        "PathRAG must not find path over tombstoned edge post-superseding"
    );
}

#[tokio::test]
async fn test_pathrag_ignores_superseded_edges() {
    let dir = tempdir().unwrap();
    let db = MemFuse::open(dir.path()).await.unwrap();
    let col = db.collection("default").await.unwrap();

    let dummy_emb = vec![0.1f32; 768];

    // Multi-hop pipeline: doc-1 -> doc-2 -> doc-3
    db.insert("doc-1", &dummy_emb, Some(serde_json::json!({"text": "Doc 1"})))
        .await
        .unwrap();
    db.insert("doc-2", &dummy_emb, Some(serde_json::json!({"text": "Doc 2"})))
        .await
        .unwrap();
    db.insert("doc-3", &dummy_emb, Some(serde_json::json!({"text": "Doc 3"})))
        .await
        .unwrap();
    db.insert("doc-4", &dummy_emb, Some(serde_json::json!({"text": "Doc 4"})))
        .await
        .unwrap();

    let _doc1_id = DocId::from_key("doc-1").unwrap();
    let doc2_id = DocId::from_key("doc-2").unwrap();
    let _doc3_id = DocId::from_key("doc-3").unwrap();
    let doc4_id = DocId::from_key("doc-4").unwrap();

    col.relate("doc-1", "doc-2", "leads_to").await.unwrap();
    col.relate("doc-2", "doc-3", "leads_to").await.unwrap();

    let eid1 = EntityId::from_key("doc-1").unwrap();
    let _eid2 = EntityId::from_key("doc-2").unwrap();
    let eid3 = EntityId::from_key("doc-3").unwrap();

    // Edge (eid2, eid3) is automatically registered for doc2_id via relate()
    let engine = PathRAGEngine::with_defaults(col.graph_index());
    assert!(engine.find_path(eid1, eid3).is_some());

    // doc-4 supersedes doc-2
    col.link_memories(doc4_id, doc2_id, LinkRelation::Supersedes)
        .await
        .unwrap();

    // Edge doc-2 -> doc-3 must be tombstoned, making doc-1 -> doc-3 path unreachable
    assert!(
        engine.find_path(eid1, eid3).is_none(),
        "PathRAG must ignore edge whose source document was superseded"
    );
}
