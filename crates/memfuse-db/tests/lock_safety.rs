use memfuse_core::Result;
use memfuse_db::{MemFuse, MemFuseConfig};
use std::sync::Arc;
use tempfile::tempdir;

#[tokio::test]
async fn test_concurrent_lock_safety_no_deadlock() -> Result<()> {
    let dir = tempdir().map_err(|e| memfuse_core::MemFuseError::InvalidInput(e.to_string()))?;
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };
    let db = Arc::new(MemFuse::open_with_config(dir.path(), config).await?);

    // Timeout safety net to guarantee test fails after 5 seconds instead of hanging/deadlocking
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let mut handles = Vec::new();

        for i in 0..10 {
            let db_clone = Arc::clone(&db);
            let handle = tokio::spawn(async move {
                let col_name = format!("collection_{}", i % 3);
                let col = db_clone.collection(&col_name).await.unwrap();

                let id = format!("doc_{}", i);
                let embedding = vec![0.1 * (i as f32), 0.2, 0.3, 0.4];

                col.insert(&id, &embedding, Some(serde_json::json!({"step": i})))
                    .await
                    .unwrap();

                let res = col.get(&id).await.unwrap();
                assert!(res.is_some());

                let search_res = col
                    .query()
                    .embedding(&embedding)
                    .k(2)
                    .execute()
                    .await
                    .unwrap();
                assert!(!search_res.is_empty());
            });
            handles.push(handle);
        }

        for h in handles {
            h.await.unwrap();
        }
    })
    .await
    .expect("Concurrent lock safety test timed out - potential deadlock detected!");

    Ok(())
}
