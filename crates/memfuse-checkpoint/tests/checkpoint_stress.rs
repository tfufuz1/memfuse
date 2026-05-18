// AGENT:12 STATUS:DONE
// ANCHOR:INTEGRATION:STRESS-002 AGENT:12 DATE:2026-05-22
use memfuse_checkpoint::CheckpointManager;
use memfuse_core::{StorageEngine, TxId};
use memfuse_store::LsmStorage;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::task::JoinHandle;

#[tokio::test(flavor = "multi_thread")]
async fn test_checkpoint_stress_concurrent_ops() {
    let tmp = TempDir::new().expect("failed to create temp dir");
    let storage = Arc::new(
        LsmStorage::new(memfuse_store::LsmConfig {
            path: tmp.path().to_path_buf(),
            ..Default::default()
        })
        .await
        .expect("failed to open storage"),
    );
    let manager = Arc::new(CheckpointManager::new(storage.clone()));

    let num_writer_tasks = 5;
    let num_checkpoint_tasks = 5;
    let ops_per_task = 100;

    let mut handles: Vec<JoinHandle<()>> = Vec::new();

    // Writer tasks
    for t in 0..num_writer_tasks {
        let storage = storage.clone();
        handles.push(tokio::spawn(async move {
            for i in 0..ops_per_task {
                let tx = TxId::new((t * ops_per_task + i + 1) as u64);
                let key = format!("key-{}-{}", t, i);
                storage.put(tx, key.as_bytes(), b"value").await.expect("put failed");
                storage.commit(tx).await.expect("commit failed");

                if i % 10 == 0 {
                    storage.force_flush().await.expect("flush failed");
                }
            }
        }));
    }

    // Checkpoint tasks
    for t in 0..num_checkpoint_tasks {
        let manager = manager.clone();
        handles.push(tokio::spawn(async move {
            for i in 0..ops_per_task {
                let name = format!("cp-{}-{}", t, i);
                let cp = manager.create_checkpoint(&name).await.expect("create checkpoint failed");

                // Keep it for a bit then drop
                tokio::task::yield_now().await;

                manager.drop_checkpoint(&cp).await.expect("drop checkpoint failed");
            }
        }));
    }

    for h in handles {
        h.await.expect("task failed");
    }
}
