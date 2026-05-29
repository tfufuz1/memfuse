use memfuse_checkpoint::PersistentCheckpointStore;
use memfuse_core::{StorageEngine, TxId};
use memfuse_store::{LsmConfig, LsmStorage};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::task::JoinHandle;

// ANCHOR:INTEGRATION:CHECKPOINT-STRESS STATUS:DONE AGENT:12 DATE:2026-06-20
// Stress test for concurrent checkpoint creation and deletion during active writes.
#[tokio::test(flavor = "multi_thread")]
async fn test_checkpoint_concurrency_stress() {
    let tmp = TempDir::new().expect("failed to create temp dir"); // expect #[cfg(test)]
    let lsm_config = LsmConfig {
        path: tmp.path().to_path_buf(),
        ..Default::default()
    };
    let storage = Arc::new(
        LsmStorage::new(lsm_config)
            .await
            .expect("failed to open storage"), // expect #[cfg(test)]
    );
    let manager = Arc::new(PersistentCheckpointStore::new(storage.clone()));

    let num_writer_tasks = 5;
    let num_checkpoint_tasks = 2;
    let ops_per_task = 100;

    let mut handles: Vec<JoinHandle<()>> = Vec::new();

    // Writer tasks
    for t in 0..num_writer_tasks {
        let storage = storage.clone();
        handles.push(tokio::spawn(async move {
            for i in 0..ops_per_task {
                let tx_id = (t * ops_per_task + i + 1) as u64;
                let tx = TxId::new(tx_id);
                let key = format!("task-{}-key-{}", t, i);
                let val = format!("val-{}", i);
                storage
                    .put(tx, key.as_bytes(), val.as_bytes())
                    .await
                    .expect("put failed"); // expect #[cfg(test)]
                storage.commit(tx).await.expect("commit failed"); // expect #[cfg(test)]

                if i % 10 == 0 {
                    storage.force_flush().await.expect("flush failed"); // expect #[cfg(test)]
                }
            }
        }));
    }

    // Checkpoint tasks
    for t in 0..num_checkpoint_tasks {
        let manager = manager.clone();
        handles.push(tokio::spawn(async move {
            for i in 0..(ops_per_task / 10) {
                let name = format!("cp-{}-{}", t, i);
                let cp = manager
                    .create_checkpoint(&name, "default", 0, TxId::new(0), serde_json::json!({}))
                    .await
                    .expect("create checkpoint failed"); // expect #[cfg(test)]

                // Keep it for a bit
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;

                manager
                    .drop_checkpoint(&cp.name)
                    .await
                    .expect("drop checkpoint failed"); // expect #[cfg(test)]
            }
        }));
    }

    for h in handles {
        h.await.expect("task panicked"); // expect #[cfg(test)]
    }

    // Final sanity check
    let last_seq = storage.last_seq_no().await.expect("get last seq"); // expect #[cfg(test)]
    assert!(last_seq > 0);
}
