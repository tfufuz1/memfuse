use memfuse_db::{MemFuse, MemFuseConfig};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::task::JoinHandle;

// ANCHOR:INTEGRATION:FULL-STACK-STRESS STATUS:DONE AGENT:12 DATE:2026-05-23
// Full stack stress test simulating concurrent ingestion, search, and collection management.
#[tokio::test(flavor = "multi_thread")]
async fn test_full_stack_stress_concurrency() {
    let tmp = TempDir::new().expect("failed to create temp dir");
    let db_path = tmp.path().to_path_buf();

    let config = MemFuseConfig {
        dimension: 128,
        ..Default::default()
    };

    let db = Arc::new(
        MemFuse::open_with_config(&db_path, config)
            .await
            .expect("open db"),
    );

    let num_tasks = 10;
    let ops_per_task = 50;
    let mut handles: Vec<JoinHandle<()>> = Vec::new();

    for t in 0..num_tasks {
        let db = db.clone();
        handles.push(tokio::spawn(async move {
            let col_name = format!("col-{}", t % 3);
            let col = db.collection(&col_name).await.expect("get col");

            for i in 0..ops_per_task {
                let id = format!("task-{}-doc-{}", t, i);
                let vec = vec![i as f32; 128];

                // 1. Insert
                col.insert(&id, &vec, Some(json!({"t": t, "i": i})))
                    .await
                    .expect("insert");

                // 2. Random Get
                if i % 5 == 0 {
                    let res = col.get(&id).await.expect("get");
                    assert!(res.is_some());
                }

                // 3. Search (Simulated)
                if i % 10 == 0 {
                    let _results = col.search(&vec, 5).await.expect("search");
                }
            }
        }));
    }

    for h in handles {
        h.await.expect("task panicked");
    }

    // Verify collection stats
    for t in 0..3 {
        let col_name = format!("col-{}", t);
        let col = db.collection(&col_name).await.expect("get col");
        let stats = col.stats().await.expect("stats");
        assert!(stats.num_vectors > 0);
    }

    let db_owned = Arc::try_unwrap(db)
        .map_err(|_| "Arc still has references")
        .expect("last reference");
    db_owned.close().await.expect("close db");
}

#[tokio::test]
async fn test_full_stack_document_lifecycle() {
    let tmp = TempDir::new().expect("failed to create temp dir");
    let db = MemFuse::open(&tmp.path()).await.expect("open db");
    let col = db.collection("test").await.expect("col");

    let id = "doc-1";
    let vec = vec![1.0; 1536]; // Default dimension
    let meta = json!({"status": "active"});

    // Create
    col.insert(id, &vec, Some(meta.clone()))
        .await
        .expect("insert");

    // Read
    let doc = col.get(id).await.expect("get").unwrap();
    assert_eq!(doc.id, id);
    assert_eq!(doc.metadata.unwrap(), meta);

    // Update
    let meta2 = json!({"status": "updated"});
    col.insert(id, &vec, Some(meta2.clone()))
        .await
        .expect("update");
    let doc2 = col.get(id).await.expect("get updated").unwrap();
    assert_eq!(doc2.metadata.unwrap(), meta2);

    // Delete
    col.delete(id).await.expect("delete");
    assert!(col.get(id).await.expect("get deleted").is_none());
}
