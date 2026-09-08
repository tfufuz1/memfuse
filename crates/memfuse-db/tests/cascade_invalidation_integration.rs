use memfuse_core::{DocId, Edge, Entity, EntityId, GraphIndex, TxId};
use memfuse_db::{execute_consolidation_pass, ConsolidationConfig, MemFuse, MemFuseConfig};
use memfuse_graph::path_rag::PathRAGEngine;
use tempfile::tempdir;

#[tokio::test]
async fn test_consolidation_pass_cascades_edge_invalidation() {
    let dir = tempdir().expect("Failed to create temp directory");
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };
    let db = MemFuse::open_with_config(dir.path(), config)
        .await
        .expect("Failed to open DB");
    let collection = db
        .collection("cascade_integration_test")
        .await
        .expect("Failed to open collection");

    let duplicate_emb = vec![1.0, 0.0, 0.0, 0.0];

    // a. Insert 2 near-duplicate documents
    let doc1_key = "doc1_old";
    let doc2_key = "doc2_new";

    collection
        .insert(
            doc1_key,
            &duplicate_emb,
            Some(serde_json::json!({"text": "outdated factual statement"})),
        )
        .await
        .expect("Failed to insert doc1");

    collection
        .insert(
            doc2_key,
            &duplicate_emb,
            Some(serde_json::json!({"text": "updated factual statement"})),
        )
        .await
        .expect("Failed to insert doc2");

    let doc1_id = DocId::from_key(doc1_key).expect("DocId for doc1");
    let doc2_id = DocId::from_key(doc2_key).expect("DocId for doc2");

    // Add graph entities and an edge derived from doc1 (older document)
    let node1 = EntityId::new(100);
    let node2 = EntityId::new(200);

    let graph = collection.graph_index();
    let tx1 = TxId::new(1);

    GraphIndex::add_entity(
        graph.as_ref(),
        tx1,
        Entity::new(node1, "ConceptA", "Entity"),
    )
    .await
    .expect("Failed to add entity 1");
    GraphIndex::add_entity(
        graph.as_ref(),
        tx1,
        Entity::new(node2, "ConceptB", "Entity"),
    )
    .await
    .expect("Failed to add entity 2");

    // Edge derived from doc1
    let edge_doc1 = Edge::new(node1, node2, "relates").with_source_doc_id(doc1_id);
    GraphIndex::add_edge(graph.as_ref(), tx1, edge_doc1)
        .await
        .expect("Failed to add edge");
    GraphIndex::commit(graph.as_ref(), tx1)
        .await
        .expect("Failed to commit graph transaction");

    // Verify graph neighbors and PathRAG engine find path node1 -> node2 BEFORE consolidation
    let neighbors_before = graph
        .neighbors(node1)
        .await
        .expect("Failed to get neighbors");
    assert_eq!(
        neighbors_before,
        vec![node2],
        "Expected node2 as active neighbor of node1 prior to consolidation"
    );

    let engine_before = PathRAGEngine::new(graph.as_ref(), 3, 0.5);
    let paths_before = engine_before.find_all_paths(node1);
    assert_eq!(
        paths_before.len(),
        1,
        "Expected 1 path originating from node1 prior to consolidation"
    );

    // b. Run consolidation pass over doc1 and doc2
    // doc1 (older, index 0) will be flagged as near-duplicate of doc2 (newer, index 1)
    let turns = vec![(doc1_id, duplicate_emb.clone()), (doc2_id, duplicate_emb.clone())];
    let consolidation_config = ConsolidationConfig {
        min_turns_per_segment: 1,
        max_turns_per_segment: 20,
        segment_cohesion_threshold: 0.70,
        near_duplicate_cosine_threshold: 0.95,
    };

    let result = execute_consolidation_pass(&collection, &turns, &consolidation_config)
        .await
        .expect("execute_consolidation_pass failed");

    // Verify consolidation pass result
    assert_eq!(
        result.duplicates_tombstoned,
        vec![doc1_id],
        "Older doc1 must be in duplicates_tombstoned"
    );
    assert_eq!(
        result.cascade_edge_tombstones_needed,
        vec![doc1_id],
        "Older doc1 must be in cascade_edge_tombstones_needed"
    );
    assert!(
        result.cascade_errors.is_empty(),
        "No errors expected during edge invalidation cascade"
    );

    // c. Verify that edge from node1 to node2 is tombstoned/inactive in CsrGraph
    let neighbors_after = graph
        .neighbors(node1)
        .await
        .expect("Failed to get neighbors");
    assert!(
        neighbors_after.is_empty(),
        "Active neighbors of node1 must be empty in CsrGraph after consolidation tombstoning"
    );

    // d. Verify that PathRAG search path no longer traverses the tombstoned edge
    let engine_after = PathRAGEngine::new(graph.as_ref(), 3, 0.5);
    let paths_after = engine_after.find_all_paths(node1);
    assert!(
        paths_after.is_empty(),
        "PathRAGEngine must return empty paths from node1 after edge tombstoning"
    );
}
