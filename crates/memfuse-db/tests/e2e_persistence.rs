use memfuse_db::{MemFuse, MemFuseConfig};
use tempfile::TempDir;
use serde_json::json;

#[tokio::test]
async fn test_e2e_persistence_across_restarts() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().to_path_buf();

    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };

    // 1. Initial Phase: Create collections and insert data
    {
        let db = MemFuse::open_with_config(&db_path, config.clone()).await.expect("Failed to open DB");

        // Default collection
        db.insert("doc-default", &[1.0, 0.0, 0.0, 0.0], Some(json!({"source": "default"}))).await.unwrap();

        // Named collection 'alpha'
        let col_alpha = db.collection("alpha").await.unwrap();
        col_alpha.insert("doc-alpha", &[0.0, 1.0, 0.0, 0.0], Some(json!({"source": "alpha"}))).await.unwrap();

        // Named collection 'beta'
        let col_beta = db.collection("beta").await.unwrap();
        col_beta.insert("doc-beta", &[0.0, 0.0, 1.0, 0.0], Some(json!({"source": "beta"}))).await.unwrap();

        assert_eq!(db.list_collections().await.unwrap().len(), 3);

        // Ensure data is flushed to disk if possible, though LSM should handle it via WAL
    }

    // 2. Restart Phase: Re-open the database
    {
        let db = MemFuse::open_with_config(&db_path, config).await.expect("Failed to re-open DB");

        let collections = db.list_collections().await.unwrap();
        assert!(collections.contains(&"default".to_string()));
        assert!(collections.contains(&"alpha".to_string()));
        assert!(collections.contains(&"beta".to_string()));

        // Verify 'default' collection data
        let doc_default = db.get("doc-default").await.unwrap().expect("doc-default missing");
        assert_eq!(doc_default.metadata.unwrap()["source"], "default");

        let search_default = db.search(&[1.0, 0.0, 0.0, 0.0], 1).await.unwrap();
        assert_eq!(search_default[0].id, "doc-default");

        // Verify 'alpha' collection data
        let col_alpha = db.collection("alpha").await.unwrap();
        let doc_alpha = col_alpha.get("doc-alpha").await.unwrap().expect("doc-alpha missing");
        assert_eq!(doc_alpha.metadata.unwrap()["source"], "alpha");

        let search_alpha = col_alpha.search(&[0.0, 1.0, 0.0, 0.0], 1).await.unwrap();
        assert_eq!(search_alpha[0].id, "doc-alpha");

        // Verify 'beta' collection data
        let col_beta = db.collection("beta").await.unwrap();
        let doc_beta = col_beta.get("doc-beta").await.unwrap().expect("doc-beta missing");
        assert_eq!(doc_beta.metadata.unwrap()["source"], "beta");

        let search_beta = col_beta.search(&[0.0, 0.0, 1.0, 0.0], 1).await.unwrap();
        assert_eq!(search_beta[0].id, "doc-beta");
    }
}
