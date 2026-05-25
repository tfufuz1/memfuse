use memfuse_core::{DocId, StorageEngine, VectorIndex};
use memfuse_db::{Collection, MemFuse};
use memfuse_index::{HnswConfig, HnswIndex};
use memfuse_store::{LsmConfig, LsmStorage};
use serde_json::json;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use tempfile::TempDir;

async fn setup_collection(
    path: &std::path::Path,
    dim: usize,
) -> (Collection, Arc<LsmStorage>, Arc<HnswIndex>) {
    let lsm_config = LsmConfig {
        path: path.to_path_buf(),
        ..Default::default()
    };
    let storage = Arc::new(LsmStorage::new(lsm_config).await.unwrap()); // unwrap

    let hnsw_config = HnswConfig {
        dimension: dim,
        ..Default::default()
    };
    let index = Arc::new(HnswIndex::new(hnsw_config));
    let next_tx = Arc::new(AtomicU64::new(1));

    let col = Collection::new(
        "test_col".to_string(),
        storage.clone(),
        index.clone(),
        next_tx,
        dim,
    );

    (col, storage, index)
}

#[tokio::test]
async fn test_manual_rollback() {
    let tmp = TempDir::new().unwrap(); // unwrap
    let dim = 4;
    let (col, storage, index) = setup_collection(tmp.path(), dim).await;

    let db_tx = col.begin_transaction();
    let tx_id = db_tx.tx_id;
    let doc_id = DocId::from_key("manual_rollback").unwrap(); // unwrap
    let embedding = vec![1.0, 0.0, 0.0, 0.0];

    // Manually put into storage and index using the transaction ID
    storage
        .put(tx_id, b"manual_rollback", b"should be rolled back")
        .await
        .unwrap(); // unwrap
    index.insert(tx_id, doc_id, &embedding).await.unwrap(); // unwrap

    // Verify it's staged but not committed yet (isolation)
    assert!(storage.get(b"manual_rollback").await.unwrap().is_none()); // unwrap
    let search_res = index.search(&embedding, 1).await.unwrap(); // unwrap
    assert!(search_res.is_empty());

    // Rollback
    db_tx.rollback().await.unwrap(); // unwrap

    // Verify it's still not there
    assert!(storage.get(b"manual_rollback").await.unwrap().is_none()); // unwrap
    let search_res = index.search(&embedding, 1).await.unwrap(); // unwrap
    assert!(search_res.is_empty());
}

#[tokio::test]
async fn test_concurrent_rollback_contention() {
    let tmp = TempDir::new().unwrap(); // unwrap
    let dim = 4;
    let (col, storage, index) = setup_collection(tmp.path(), dim).await;
    let col = Arc::new(col);

    let num_tasks = 20;
    let mut tasks = Vec::new();

    for i in 0..num_tasks {
        let col = col.clone();
        let storage = storage.clone();
        let index = index.clone();

        tasks.push(tokio::spawn(async move {
            let id = format!("doc-{}", i);
            let doc_id = DocId::from_key(&id).unwrap(); // unwrap
            let embedding = vec![i as f32, 0.0, 0.0, 0.0];

            let db_tx = col.begin_transaction();
            let tx_id = db_tx.tx_id;

            storage
                .put(tx_id, id.as_bytes(), id.as_bytes())
                .await
                .unwrap(); // unwrap
            index.insert(tx_id, doc_id, &embedding).await.unwrap(); // unwrap

            // Randomly commit or rollback
            if i % 2 == 0 {
                db_tx.commit().await.unwrap(); // unwrap
                true // committed
            } else {
                db_tx.rollback().await.unwrap(); // unwrap
                false // rolled back
            }
        }));
    }

    let mut committed_count = 0;
    for task in tasks {
        if task.await.unwrap() {
            // unwrap
            // unwrap
            // unwrap
            // unwrap
            committed_count += 1;
        }
    }

    assert_eq!(committed_count, num_tasks / 2);

    // Verify final state
    let mut found_count = 0;
    for i in 0..num_tasks {
        let id = format!("doc-{}", i);
        let val = storage.get(id.as_bytes()).await.unwrap(); // unwrap
        if i % 2 == 0 {
            assert!(val.is_some(), "Doc {} should be committed", i);
            found_count += 1;
        } else {
            assert!(val.is_none(), "Doc {} should be rolled back", i);
        }
    }
    assert_eq!(found_count, committed_count);

    // Verify index state
    assert_eq!(index.len().await, committed_count);
}

#[tokio::test]
async fn test_snapshot_stability() {
    let tmp = TempDir::new().unwrap(); // unwrap
    let config = memfuse_db::MemFuseConfig {
        dimension: 3,
        ..Default::default()
    };
    let (db, _tmp) = (
        MemFuse::open_with_config(tmp.path(), config).await.unwrap(), // unwrap
        tmp,
    );
    let col = db.collection("snapshot_test").await.unwrap(); // unwrap

    // 1. Initial state
    col.insert("base", &[0.0, 0.0, 0.0], Some(json!({"v": 0})))
        .await
        .unwrap(); // unwrap

    let snap_seq = db.last_committed_seq().await.unwrap(); // unwrap

    // 2. Modify "base" and insert "new" in a new transaction
    col.insert("base", &[1.0, 1.0, 1.0], Some(json!({"v": 1})))
        .await
        .unwrap(); // unwrap
    col.insert("new", &[0.5, 0.5, 0.5], None).await.unwrap(); // unwrap

    let _latest_seq = db.last_committed_seq().await.unwrap(); // unwrap

    // 3. Read from snapshot
    let base_snap = col.get_at_snapshot("base", snap_seq).await.unwrap(); // unwrap
    let base_snap = base_snap.expect("Base doc missing in snapshot"); // unwrap
    assert_eq!(base_snap.metadata.unwrap()["v"], 0); // unwrap

    let new_snap = col.get_at_snapshot("new", snap_seq).await.unwrap(); // unwrap
    assert!(
        new_snap.is_none(),
        "New doc should not be visible in old snapshot"
    );

    // 4. Read latest
    let base_latest = col.get("base").await.unwrap().unwrap(); // unwrap
    assert_eq!(base_latest.metadata.unwrap()["v"], 1); // unwrap
}
