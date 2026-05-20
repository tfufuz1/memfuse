//! Persistence E2E test verifying data integrity across database restarts.
// ANCHOR:INTEGRATION:E2E-002 STATUS:READY AGENT:12 DATE:2026-05-18

use memfuse_db::{MemFuse, MemFuseConfig};
use serde_json::json;
use tempfile::TempDir;

#[tokio::test]
async fn test_persistence_integrity_across_restarts() {
    let tmp = TempDir::new().expect("failed to create temp dir");
    let path = tmp.path().to_owned();
    let config = MemFuseConfig {
        dimension: 3,
        ..Default::default()
    };

    // 1. First session: Insert data and close
    {
        let db = MemFuse::open_with_config(&path, config.clone())
            .await
            .expect("failed to open db");
        let col = db.collection("persist-test").await.expect("collection failed");

        col.insert("doc-1", &[1.0, 0.0, 0.0], Some(json!({"val": 1})))
            .await
            .expect("insert 1 failed");
        col.insert("doc-2", &[0.0, 1.0, 0.0], Some(json!({"val": 2})))
            .await
            .expect("insert 2 failed");

        // Force a flush to ensure SSTables are created (though WAL should also work)
        db.inner_storage().force_flush().await.expect("flush failed");

        // db dropped here
    }

    // 2. Second session: Verify data exists
    {
        let db = MemFuse::open_with_config(&path, config.clone())
            .await
            .expect("failed to re-open db");
        let col = db.collection("persist-test").await.expect("collection failed");

        let doc1 = col.get("doc-1").await.expect("get 1 failed").expect("doc-1 missing");
        let doc2 = col.get("doc-2").await.expect("get 2 failed").expect("doc-2 missing");

        assert_eq!(doc1.metadata.unwrap()["val"], 1);
        assert_eq!(doc2.metadata.unwrap()["val"], 2);

        // Verify HNSW index is also reloaded
        let search_res = col.search(&[1.0, 0.1, 0.0], 1).await.expect("search failed");
        assert_eq!(search_res[0].id, "doc-1");

        // Add more data
        col.insert("doc-3", &[0.0, 0.0, 1.0], Some(json!({"val": 3})))
            .await
            .expect("insert 3 failed");
    }

    // 3. Third session: Verify all data including doc-3
    {
        let db = MemFuse::open_with_config(&path, config)
            .await
            .expect("failed to re-re-open db");
        let col = db.collection("persist-test").await.expect("collection failed");

        assert_eq!(col.len().await, 3);
        let doc3 = col.get("doc-3").await.expect("get 3 failed").expect("doc-3 missing");
        assert_eq!(doc3.metadata.unwrap()["val"], 3);
    }
}
