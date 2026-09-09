use memfuse_core::{Edge, Entity, EntityId, GraphIndex, PprConfig};
use memfuse_db::transaction::DbTransaction;
use memfuse_db::Collection;
use memfuse_graph::CsrGraph;
use memfuse_index::HnswIndex;
use memfuse_store::{LsmConfig, LsmStorage};
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use tempfile::tempdir;

type TestTx = DbTransaction<LsmStorage, HnswIndex>;

async fn create_test_collection(
    dir: &tempfile::TempDir,
) -> (Collection<LsmStorage, HnswIndex>, Arc<LsmStorage>) {
    let lsm_config = LsmConfig {
        path: dir.path().to_path_buf(),
        ..Default::default()
    };
    let storage = Arc::new(LsmStorage::new(lsm_config).await.unwrap());
    let index = Arc::new(
        HnswIndex::try_new(memfuse_index::HnswConfig {
            dimension: 4,
            ..Default::default()
        })
        .unwrap(),
    );
    let graph = Arc::new(CsrGraph::with_storage(storage.clone()));
    let next_tx = Arc::new(AtomicU64::new(1));

    let col = Collection::new(
        "graph_delete_test".to_string(),
        storage.clone(),
        index,
        graph,
        next_tx,
        4,
        memfuse_text::Language::English,
    );
    (col, storage)
}

#[tokio::test]
async fn test_delete_entity_removes_outgoing_edges() {
    let dir = tempdir().unwrap();
    let (col, _) = create_test_collection(&dir).await;

    let tx_id1 = col.allocate_tx().unwrap();
    let tx1: TestTx = DbTransaction::new(col.clone(), tx_id1);

    let id_a = EntityId::new(1);
    let id_b = EntityId::new(2);
    let id_c = EntityId::new(3);

    tx1.stage_graph_entity(Entity::new(id_a, "EntityA", "Concept"));
    tx1.stage_graph_entity(Entity::new(id_b, "EntityB", "Concept"));
    tx1.stage_graph_entity(Entity::new(id_c, "EntityC", "Concept"));

    tx1.stage_graph_edge(Edge::new(id_a, id_b, "relates_to"));
    tx1.stage_graph_edge(Edge::new(id_a, id_c, "relates_to"));

    tx1.commit().await.unwrap();

    // Verify initial graph state
    let graph = col.graph_index();
    let neighbors_before = graph.neighbors(id_a).await.unwrap();
    assert_eq!(neighbors_before.len(), 2);

    // Delete Entity A
    let tx_id2 = col.allocate_tx().unwrap();
    let tx2: TestTx = DbTransaction::new(col.clone(), tx_id2);
    tx2.stage_graph_entity_delete(id_a);
    tx2.commit().await.unwrap();

    // Verify PPR from A returns empty or no rank for B/C
    let ppr_res = graph
        .personalized_page_rank(&[id_a], &PprConfig::default())
        .await
        .unwrap();
    assert!(
        ppr_res.is_empty(),
        "PPR from deleted Entity A should return empty"
    );

    // Verify traversal / neighbors from A returns empty
    let neighbors_after = graph.neighbors(id_a).await.unwrap();
    assert!(
        neighbors_after.is_empty(),
        "Deleted Entity A should have no outgoing neighbors"
    );

    // Verify entity_exists for A returns false
    assert!(
        !graph.entity_exists(id_a),
        "Entity A should no longer exist"
    );
}

#[tokio::test]
async fn test_delete_entity_removes_incoming_edges() {
    let dir = tempdir().unwrap();
    let (col, _) = create_test_collection(&dir).await;

    let tx_id1 = col.allocate_tx().unwrap();
    let tx1: TestTx = DbTransaction::new(col.clone(), tx_id1);

    let id_a = EntityId::new(1);
    let id_b = EntityId::new(2);

    tx1.stage_graph_entity(Entity::new(id_a, "EntityA", "Concept"));
    tx1.stage_graph_entity(Entity::new(id_b, "EntityB", "Concept"));
    tx1.stage_graph_edge(Edge::new(id_a, id_b, "points_to"));

    tx1.commit().await.unwrap();

    let graph = col.graph_index();
    let neighbors_before = graph.neighbors(id_a).await.unwrap();
    assert_eq!(neighbors_before, vec![id_b]);

    // Delete Entity B
    let tx_id2 = col.allocate_tx().unwrap();
    let tx2: TestTx = DbTransaction::new(col.clone(), tx_id2);
    tx2.stage_graph_entity_delete(id_b);
    tx2.commit().await.unwrap();

    // Verify traversal from A finds no valid edge to B
    let neighbors_after = graph.neighbors(id_a).await.unwrap();
    assert!(
        !neighbors_after.contains(&id_b),
        "Traversal from A should no longer find edge to deleted B"
    );

    let traversal_res = graph.traverse(id_a, 1).await.unwrap();
    assert!(
        traversal_res.is_empty(),
        "Traversal from A should return no results after B is deleted"
    );
}

#[tokio::test]
async fn test_entity_delete_persists_after_restart() {
    let dir = tempdir().unwrap();
    let (col, storage) = create_test_collection(&dir).await;

    let tx_id1 = col.allocate_tx().unwrap();
    let tx1: TestTx = DbTransaction::new(col.clone(), tx_id1);

    let id_a = EntityId::new(1);
    let id_b = EntityId::new(2);

    tx1.stage_graph_entity(Entity::new(id_a, "EntityA", "Concept"));
    tx1.stage_graph_entity(Entity::new(id_b, "EntityB", "Concept"));
    tx1.stage_graph_edge(Edge::new(id_a, id_b, "points_to"));

    tx1.commit().await.unwrap();

    // Delete Entity A
    let tx_id2 = col.allocate_tx().unwrap();
    let tx2: TestTx = DbTransaction::new(col.clone(), tx_id2);
    tx2.stage_graph_entity_delete(id_a);
    tx2.commit().await.unwrap();

    // Simulate restart by reloading CsrGraph from storage
    let reloaded_graph = CsrGraph::load_from_storage(storage.as_ref())
        .await
        .unwrap();

    assert!(
        !reloaded_graph.entity_exists(id_a),
        "Deleted Entity A must not exist after storage reload"
    );
    assert!(
        reloaded_graph.entity_exists(id_b),
        "Entity B should still exist after storage reload"
    );

    let neighbors = reloaded_graph.neighbors(id_a).await.unwrap();
    assert!(
        neighbors.is_empty(),
        "Neighbors of deleted Entity A must be empty after restart"
    );
}
