use memfuse_db::{DistanceMetric, MemFuse, MemFuseConfig};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::task::JoinHandle;

#[tokio::test(flavor = "multi_thread")]
async fn test_concurrent_collection_operations() {
    let tmp = TempDir::new().expect("Failed to create temp dir"); // #[cfg(test)]
    let config = MemFuseConfig {
        dimension: 4,
        max_elements: 10000,
        distance_metric: DistanceMetric::Cosine,
        encryption_passphrase: None,
    };

    let db = Arc::new(
        MemFuse::open_with_config(tmp.path(), config)
            .await
            .expect("Failed to open DB"), // #[cfg(test)]
    );

    let num_collections = 10;
    let mut handles: Vec<JoinHandle<()>> = Vec::new();

    for i in 0..num_collections {
        let db_clone = db.clone();
        handles.push(tokio::spawn(async move {
            let col_name = format!("collection_{}", i);
            let col = db_clone
                .collection(&col_name)
                .await
                .expect("Failed to get collection"); // #[cfg(test)]

            for j in 0..100 {
                let id = format!("doc_{}", j);
                col.insert(&id, &[1.0, 0.0, 0.0, 0.0], None)
                    .await
                    .expect("Insert failed"); // #[cfg(test)]
            }

            assert_eq!(col.len().await, 100);
        }));
    }

    for handle in handles {
        handle.await.expect("Task failed"); // #[cfg(test)]
    }

    let collections = db.list_collections().await.expect("List failed"); // #[cfg(test)]
    assert_eq!(collections.len(), num_collections + 1); // +1 for default
}
