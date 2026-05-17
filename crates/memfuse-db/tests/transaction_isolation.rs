//! Test file for transaction isolation.
use memfuse_core::{DocId, StorageEngine, VectorIndex};
use memfuse_db::Collection;
use memfuse_index::{HnswConfig, HnswIndex};
use memfuse_store::{LsmConfig, LsmStorage};
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
    let storage = Arc::new(LsmStorage::new(lsm_config).await.unwrap());

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
    let tmp = TempDir::new().unwrap();
    let dim = 4;
    let (col, storage, index) = setup_collection(tmp.path(), dim).await;

    let db_tx = col.begin_transaction();
    let tx_id = db_tx.tx_id;
    let doc_id = DocId::from_key("manual_rollback");
    let embedding = vec![1.0, 0.0, 0.0, 0.0];

    // Manually put into storage and index using the transaction ID
    storage
        .put(tx_id, b"manual_rollback", b"should be rolled back")
        .await
        .unwrap();
    index.insert(tx_id, doc_id, &embedding).await.unwrap();

    // Verify it's staged but not committed yet (isolation)
    assert!(storage.get(b"manual_rollback").await.unwrap().is_none());
    let search_res = index.search(&embedding, 1).await.unwrap();
    assert!(search_res.is_empty());

    // Rollback
    db_tx.rollback().await.unwrap();

    // Verify it's still not there
    assert!(storage.get(b"manual_rollback").await.unwrap().is_none());
    let search_res = index.search(&embedding, 1).await.unwrap();
    assert!(search_res.is_empty());
}

#[tokio::test]
async fn test_concurrent_rollback_contention() {
    let tmp = TempDir::new().unwrap();
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
            let doc_id = DocId::from_key(&id);
            let embedding = vec![i as f32, 0.0, 0.0, 0.0];

            let db_tx = col.begin_transaction();
            let tx_id = db_tx.tx_id;

            storage
                .put(tx_id, id.as_bytes(), id.as_bytes())
                .await
                .unwrap();
            index.insert(tx_id, doc_id, &embedding).await.unwrap();

            // Randomly commit or rollback
            if i % 2 == 0 {
                db_tx.commit().await.unwrap();
                true // committed
            } else {
                db_tx.rollback().await.unwrap();
                false // rolled back
            }
        }));
    }

    let mut committed_count = 0;
    for task in tasks {
        if task.await.unwrap() {
            committed_count += 1;
        }
    }

    assert_eq!(committed_count, num_tasks / 2);

    // Verify final state
    let mut found_count = 0;
    for i in 0..num_tasks {
        let id = format!("doc-{}", i);
        let val = storage.get(id.as_bytes()).await.unwrap();
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
