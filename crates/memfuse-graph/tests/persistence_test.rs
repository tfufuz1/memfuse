use memfuse_core::{Entity, EntityId, GraphIndex, StorageEngine, TxId};
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
                memfuse_core::Edge::new(
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
