use memfuse_core::{Edge, Entity, EntityId, GraphIndex, StorageEngine, TxId};
use memfuse_graph::CsrGraph;
use memfuse_store::{LsmConfig, LsmStorage};
use std::sync::Arc;
use tempfile::tempdir;

#[tokio::test]
async fn test_graph_survives_restart() {
    let dir = tempdir().unwrap();
    let storage_path = dir.path().to_path_buf();

    // Phase 1: Graph öffnen, Daten einfügen, committen
    {
        let config = LsmConfig {
            path: storage_path.clone(),
            ..Default::default()
        };
        let storage = Arc::new(LsmStorage::new(config).await.unwrap());
        let graph = CsrGraph::with_storage(storage.clone());
        let tx = TxId::new(1);
        graph
            .add_entity(
                tx,
                Entity::new(EntityId::from("kunde_mueller"), "Kunde Mueller", "Kunde"),
            )
            .await
            .unwrap();
        graph
            .add_entity(
                tx,
                Entity::new(EntityId::from("produkt_123"), "Produkt 123", "Produkt"),
            )
            .await
            .unwrap();
        graph
            .add_edge(
                tx,
                Edge::new(
                    EntityId::from("kunde_mueller"),
                    EntityId::from("produkt_123"),
                    "kauft",
                )
                .with_weight(0.9),
            )
            .await
            .unwrap();
        graph.commit(tx).await.unwrap();
        storage.commit(tx).await.unwrap();
    }

    // Phase 2: Neuer Storage/Graph-Handle auf gleichem Pfad → muss Daten wiederfinden
    {
        let config = LsmConfig {
            path: storage_path,
            ..Default::default()
        };
        let storage = Arc::new(LsmStorage::new(config).await.unwrap());
        let loaded_graph = CsrGraph::load_from_storage(storage.as_ref()).await.unwrap();
        assert_eq!(loaded_graph.entity_count(), 2);

        let results = loaded_graph
            .traverse(EntityId::from("kunde_mueller"), 1)
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, EntityId::from("produkt_123"));
    }
}

#[tokio::test]
async fn test_graph_survives_reload() {
    let dir = tempdir().unwrap();
    let storage_path = dir.path().to_path_buf();

    let config = LsmConfig {
        path: storage_path.clone(),
        ..Default::default()
    };
    let storage = Arc::new(LsmStorage::new(config).await.unwrap());

    // Entitäten und Kanten einfügen
    let tx = TxId::new(1);
    let graph = CsrGraph::with_storage(storage.clone());
    graph
        .add_entity(tx, Entity::new(EntityId::from("A"), "Doc A", "Doc"))
        .await
        .unwrap();
    graph
        .add_entity(tx, Entity::new(EntityId::from("B"), "Doc B", "Doc"))
        .await
        .unwrap();
    graph
        .add_edge(
            tx,
            Edge::new(EntityId::from("A"), EntityId::from("B"), "related").with_weight(0.9),
        )
        .await
        .unwrap();
    graph.commit(tx).await.unwrap();
    storage.commit(tx).await.unwrap();

    // Graph neu laden (simuliert Neustart)
    let graph2 = CsrGraph::load_from_storage(storage.as_ref()).await.unwrap();

    // Entity-Count prüfen
    assert_eq!(
        graph2.entity_count(),
        2,
        "Geladener Graph muss 2 Entities haben"
    );

    // Edge-Count prüfen
    assert_eq!(graph2.edge_count(), 1, "Geladener Graph muss 1 Edge haben");

    // Traversal muss Ergebnis liefern
    let results = graph2.traverse(EntityId::from("A"), 2).await.unwrap();
    assert!(
        !results.is_empty(),
        "Graph muss nach Reload traversierbar sein"
    );
    assert!(
        results.iter().any(|(id, _)| *id == EntityId::from("B")),
        "Traversal muss Entity B finden, got: {:?}",
        results
    );
}

#[tokio::test]
async fn edges_survive_restart() {
    let dir = tempdir().unwrap();
    let config = LsmConfig {
        path: dir.path().to_path_buf(),
        ..Default::default()
    };
    let storage = Arc::new(LsmStorage::new(config).await.unwrap());
    let graph = CsrGraph::with_storage(storage.clone());
    let tx = TxId::new(1);
    let id_a = EntityId::from("id_a");
    let id_b = EntityId::from("id_b");

    graph
        .add_entity(tx, Entity::new(id_a, "Node A", "Type"))
        .await
        .unwrap();
    graph
        .add_entity(tx, Entity::new(id_b, "Node B", "Type"))
        .await
        .unwrap();
    graph
        .add_edge(tx, Edge::new(id_a, id_b, "relates"))
        .await
        .unwrap();
    graph.commit(tx).await.unwrap();
    storage.commit(tx).await.unwrap();

    // Simuliere Neustart: neuer Graph aus derselben Storage
    let graph2 = CsrGraph::load_from_storage(storage.as_ref()).await.unwrap();
    let neighbors = graph2.neighbors(id_a).await.unwrap();
    assert!(neighbors.contains(&id_b), "Kante nicht persistiert!");
}
