use memfuse_core::types::domain::LinkRelation;
use memfuse_core::{DocId, Edge, Entity, EntityId, GraphIndex, TxId};
use memfuse_db::MemFuse;
use memfuse_graph::path_rag::PathRAGEngine;
use tempfile::tempdir;

#[tokio::test]
async fn test_e2e_supersedes_cascade_invalidates_graph_edges() {
    let dir = tempdir().unwrap();
    let db = MemFuse::open(dir.path()).await.unwrap();
    let col = db.collection("default").await.unwrap();

    let dummy_emb = vec![0.1f32; col.dimension()];

    // Insert doc1 (old) and doc2 (new) and doc3 (unrelated)
    col.insert("doc1", &dummy_emb, Some(serde_json::json!({"text": "outdated factual statement"})))
        .await
        .unwrap();
    col.insert("doc2", &dummy_emb, Some(serde_json::json!({"text": "updated factual statement"})))
        .await
        .unwrap();
    col.insert("doc3", &dummy_emb, Some(serde_json::json!({"text": "separate unrelated fact"})))
        .await
        .unwrap();

    let doc1_id = DocId::from_key("doc1").unwrap();
    let doc2_id = DocId::from_key("doc2").unwrap();
    let doc3_id = DocId::from_key("doc3").unwrap();

    let node1 = EntityId::new(10);
    let node2 = EntityId::new(20);
    let node3 = EntityId::new(30);

    let graph = col.graph_index();
    let tx1 = TxId::new(1);

    // Add entities and edges derived from doc1 and doc3 into graph
    GraphIndex::add_entity(graph.as_ref(), tx1, Entity::new(node1, "Entity1", "Concept")).await.unwrap();
    GraphIndex::add_entity(graph.as_ref(), tx1, Entity::new(node2, "Entity2", "Concept")).await.unwrap();
    GraphIndex::add_entity(graph.as_ref(), tx1, Entity::new(node3, "Entity3", "Concept")).await.unwrap();

    // Edge 1 derived from doc1 (outdated)
    let edge_doc1 = Edge::new(node1, node2, "supports").with_source_doc_id(doc1_id);
    // Edge 2 derived from doc3 (unaffected)
    let edge_doc3 = Edge::new(node2, node3, "relates").with_source_doc_id(doc3_id);

    GraphIndex::add_edge(graph.as_ref(), tx1, edge_doc1).await.unwrap();
    GraphIndex::add_edge(graph.as_ref(), tx1, edge_doc3).await.unwrap();
    GraphIndex::commit(graph.as_ref(), tx1).await.unwrap();

    // Verify both edges are active prior to superseding
    let engine_before = PathRAGEngine::new(graph.as_ref(), 3, 0.5);
    let paths_before = engine_before.find_all_paths(node1);
    assert_eq!(paths_before.len(), 2, "Expected 2 paths (node1->node2 and node1->node2->node3) before invalidation");

    // doc2 supersedes doc1
    col.link_memories(doc2_id, doc1_id, LinkRelation::Supersedes)
        .await
        .unwrap();

    // Verify edge derived from doc1 is now tombstoned and excluded from PathRAGEngine
    let engine_after = PathRAGEngine::new(graph.as_ref(), 3, 0.5);
    let paths_after = engine_after.find_all_paths(node1);
    assert!(paths_after.is_empty(), "Paths originating from node1 via doc1 edge must be empty after cascade invalidation");

    // Edge derived from doc3 (node2->node3) remains active
    let paths_node2 = engine_after.find_all_paths(node2);
    assert_eq!(paths_node2.len(), 1, "Edge derived from doc3 must remain active");

    // Calling link_memories again for doc1 should be idempotent
    let second_link_res = col.link_memories(doc2_id, doc1_id, LinkRelation::Supersedes).await;
    assert!(second_link_res.is_ok(), "Idempotent link_memories call must succeed without error");
}
