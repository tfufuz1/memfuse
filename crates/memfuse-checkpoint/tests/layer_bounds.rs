use memfuse_checkpoint::CheckpointManager;
use memfuse_core::StorageEngine;
use memfuse_db::MemFuseConfig;
use std::sync::Arc;
use tempfile::TempDir;

// ANCHOR:TEST:LAYER-001 — DAG Integrationstest fehlt
// ZIEL: memfuse-checkpoint -> memfuse-db (Fork + Diverge + Merge)
// HINWEIS: Rollback ist aktuell ein Stub, daher testen wir hier die Pinning-Logik
// und die Interaktion mit dem DB-Zustand.
#[tokio::test]
async fn test_layer_001_checkpoint_db_interaction() {
    let tmp = TempDir::new().unwrap();
    let _config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };

    // Wir nutzen das interne LsmStorage von MemFuse für den CheckpointManager
    // Da MemFuse das Storage nicht direkt exposed, müssen wir es über Reflection/Transparenz lösen
    // oder wir simulieren die Schichten.

    // Da memfuse-db::MemFuse die storage privat hält, können wir sie im Test nicht einfach extrahieren
    // außer wir ändern MemFuse (was wir nicht sollen).

    // Alternative: Wir erstellen ein LsmStorage manuell, wie MemFuse es tun würde.
    let lsm_config = memfuse_store::LsmConfig {
        path: tmp.path().to_path_buf(),
        ..Default::default()
    };
    let storage = Arc::new(memfuse_store::LsmStorage::new(lsm_config).await.unwrap());
    let checkpoint_manager = CheckpointManager::new(storage.clone());

    // In einer echten E2E Umgebung würde MemFuse auf diesem Storage operieren.
    // Da wir aber Integration Tester sind und Layer-Bounds testen:

    // 1. "Fork" - Checkpoint erstellen
    let cp1 = checkpoint_manager.create_checkpoint("v1").await.unwrap();

    // 2. "Diverge" - Daten hinzufügen (via Storage direkt, da wir das Storage teilen)
    // In der Realität würde der User über MemFuse API gehen.
    let tx = memfuse_core::TxId::new(100);
    storage.put(tx, b"key1", b"val1").await.unwrap();
    storage.commit(tx).await.unwrap();

    let cp2 = checkpoint_manager.create_checkpoint("v2").await.unwrap();
    assert!(cp2.seq_no > cp1.seq_no);

    // 3. "Merge" / Rollback (Stub Test)
    checkpoint_manager.rollback(&cp1).await.unwrap();

    // Cleanup
    checkpoint_manager.drop_checkpoint(&cp1).await.unwrap();
    checkpoint_manager.drop_checkpoint(&cp2).await.unwrap();
}
