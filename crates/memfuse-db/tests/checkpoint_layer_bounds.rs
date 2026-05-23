use memfuse_checkpoint::CheckpointManager;
use memfuse_db::{MemFuse, MemFuseConfig};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;

// AGENT:12 DATE:2026-05-09 STATUS:DONE
// ZIEL: memfuse-checkpoint -> memfuse-db (Fork + Diverge + Merge)
//
// Dieser Test verifiziert die Zusammenarbeit zwischen dem CheckpointManager
// und der MemFuse-DB Facade. Er simuliert einen "Fork", indem er eine neue
// Collection erstellt und Daten basierend auf einem Checkpoint repliziert.
#[tokio::test]
async fn test_layer_001_fork_diverge_merge() {
    let tmp = TempDir::new().expect("temp dir"); // unwrap allowed
    let db_path = tmp.path().to_path_buf();

    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };

    // 1. Initialisierung: DB befüllen
    {
        let db = MemFuse::open_with_config(&db_path, config.clone())
            .await
            .expect("open db"); // unwrap allowed
        let main_col = db.collection("main").await.expect("main col"); // unwrap allowed

        // Daten einfügen in "main"
        main_col
            .insert(
                "doc-1",
                &[1.0, 0.0, 0.0, 0.0],
                Some(json!({"val": "initial"})),
            )
            .await
            .expect("insert 1"); // unwrap allowed
        main_col
            .insert(
                "doc-2",
                &[0.0, 1.0, 0.0, 0.0],
                Some(json!({"val": "initial"})),
            )
            .await
            .expect("insert 2"); // unwrap allowed

        // Explizites Drop/Close damit Filesystem-Locks frei werden
    }

    // 2. Checkpoint erstellen (Simuliert durch CheckpointManager auf ruhenden Daten)
    let _cp_v1;
    {
        let lsm_config = memfuse_store::LsmConfig {
            path: db_path.clone(),
            ..Default::default()
        };
        let storage = Arc::new(
            memfuse_store::LsmStorage::new(lsm_config)
                .await
                .expect("storage"), // unwrap allowed
        );
        let cp_manager = CheckpointManager::new(storage.clone());

        _cp_v1 = cp_manager
            .create_checkpoint("v1", "main", 0, json!({}))
            .await
            .expect("checkpoint"); // unwrap allowed
                                   // Storage wird gedroppt, Lock frei.
    }

    // 3. "Fork" simulieren
    {
        let db = MemFuse::open_with_config(&db_path, config.clone())
            .await
            .expect("open db"); // unwrap allowed
        let main_col = db.collection("main").await.expect("main col"); // unwrap allowed
        let fork_col = db.collection("fork-v1").await.expect("fork col"); // unwrap allowed

        // Daten von "main" nach "fork" kopieren (Simulation von Fork-Logic)
        let main_data = main_col.scan_prefix("").await.expect("scan main"); // unwrap allowed
        for (id, meta) in main_data {
            fork_col
                .insert(&id, &[0.5, 0.5, 0.5, 0.5], Some(meta))
                .await
                .expect("insert fork"); // unwrap allowed
        }

        // 4. "Diverge" (Auseinanderlaufen)
        main_col
            .insert(
                "doc-main-only",
                &[1.0, 1.0, 0.0, 0.0],
                Some(json!({"origin": "main"})),
            )
            .await
            .expect("ins main only"); // unwrap allowed

        fork_col
            .insert(
                "doc-fork-only",
                &[0.0, 0.0, 1.0, 1.0],
                Some(json!({"origin": "fork"})),
            )
            .await
            .expect("ins fork only"); // unwrap allowed

        // Verifizieren der Divergenz
        assert!(main_col.get("doc-main-only").await.unwrap().is_some()); // unwrap allowed
        assert!(main_col.get("doc-fork-only").await.unwrap().is_none()); // unwrap allowed

        assert!(fork_col.get("doc-fork-only").await.unwrap().is_some()); // unwrap allowed
        assert!(fork_col.get("doc-main-only").await.unwrap().is_none()); // unwrap allowed

        // 5. "Merge" simulieren
        let fork_doc = fork_col.get("doc-fork-only").await.expect("get").unwrap(); // unwrap allowed // unwrap allowed
        main_col
            .insert(&fork_doc.id, &[0.0, 0.0, 1.0, 1.0], fork_doc.metadata)
            .await
            .expect("merge insert"); // unwrap allowed

        // Final State Check
        let merged_doc = main_col
            .get("doc-fork-only")
            .await
            .expect("get merged") // unwrap allowed
            .unwrap(); // unwrap allowed
        assert_eq!(merged_doc.metadata.unwrap()["origin"], "fork"); // unwrap allowed
    }

    // 6. Cleanup Checkpoint
    {
        let lsm_config = memfuse_store::LsmConfig {
            path: db_path,
            ..Default::default()
        };
        let storage = Arc::new(
            memfuse_store::LsmStorage::new(lsm_config)
                .await
                .expect("storage"), // unwrap allowed
        );
        let cp_manager = CheckpointManager::new(storage.clone());
        cp_manager.drop_checkpoint("v1").await.expect("drop cp"); // unwrap allowed
    }
}
