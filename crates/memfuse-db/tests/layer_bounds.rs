// ANCHOR:TEST:LAYER-002 — DAG Integrationstest
// WP:NONE PRIO:3 NEEDS:NONE
// AGENT:07 DATE:2026-05-09 STATUS:DONE
//! Verifies that memfuse-db correctly uses trait objects for storage and index.

use memfuse_db::{MemFuse, MemFuseConfig};
use tempfile::TempDir;

#[tokio::test]
async fn test_db_layer_bounds_orchestration() {
    let tmp = TempDir::new().unwrap();
    let config = MemFuseConfig {
        dimension: 128,
        ..Default::default()
    };

    // Test opening DB and creating a collection
    let db = MemFuse::open_with_config(tmp.path().to_str().unwrap(), config).await.unwrap();
    let col = db.collection("test_bounds").await.unwrap();

    // Insert should work (orchestrates store + index)
    col.insert("doc1", &[0.1; 128], None).await.unwrap();

    // Search should work
    let results = col.search(&[0.1; 128], 10).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "doc1");
}
